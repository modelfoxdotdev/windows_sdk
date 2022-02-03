use digest::Digest;
use duct::cmd;
use futures::{future::join_all, StreamExt};
use indexmap::IndexMap;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::Sha256;
use std::{
	collections::{HashMap, HashSet},
	path::PathBuf,
};
use tempfile::tempdir;
use tokio::io::AsyncWriteExt;
use url::Url;
use walkdir::WalkDir;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Channel {
	#[serde(rename = "channelItems")]
	channel_items: Vec<ChannelItem>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ChannelItem {
	id: String,
	version: String,
	#[serde(rename = "type")]
	ty: ChannelItemType,
	payloads: Option<Vec<Payload>>,
}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
enum ChannelItemType {
	Manifest,
	#[serde(other)]
	Other,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
	#[serde(rename = "manifestVersion")]
	pub manifest_version: String,
	#[serde(rename = "engineVersion")]
	pub engine_version: String,
	pub packages: Vec<Package>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Package {
	pub id: String,
	pub version: String,
	#[serde(rename = "type")]
	pub ty: PackageType,
	#[serde(default)]
	pub dependencies: IndexMap<String, Dependency>,
	#[serde(default)]
	pub payloads: Vec<Payload>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(from = "DependencyRaw")]
pub struct Dependency {
	pub version: String,
	pub ty: Option<DependencyType>,
	pub chip: Option<DependencyChip>,
}

impl From<DependencyRaw> for Dependency {
	fn from(value: DependencyRaw) -> Self {
		match value {
			DependencyRaw::String(version) => Dependency {
				version,
				ty: Default::default(),
				chip: Default::default(),
			},
			DependencyRaw::Map { version, ty, chip } => Dependency { version, ty, chip },
		}
	}
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum DependencyRaw {
	String(String),
	Map {
		version: String,
		#[serde(rename = "type")]
		ty: Option<DependencyType>,
		chip: Option<DependencyChip>,
	},
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum DependencyType {
	Optional,
	Recommended,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum DependencyChip {
	#[serde(rename = "x86", alias = "X86")]
	X86,
	#[serde(rename = "x64", alias = "X64")]
	X64,
	#[serde(rename = "arm")]
	Arm,
	#[serde(rename = "arm64")]
	Arm64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PackageType {
	Component,
	Exe,
	Group,
	Msi,
	Msu,
	Nupkg,
	Product,
	Vsix,
	WindowsFeature,
	Workload,
	Zip,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Payload {
	#[serde(rename = "fileName")]
	pub file_name: String,
	#[serde(with = "hex::serde")]
	pub sha256: [u8; 32],
	pub size: u64,
	pub url: Url,
}

pub fn get_manifest_url(major_version: String) {
	let channel_url = format!("https://aka.ms/vs/{}/release/channel", major_version);
	let channel: Channel = reqwest::blocking::get(channel_url).unwrap().json().unwrap();
	let manifest_payload = channel
		.channel_items
		.iter()
		.find(|channel_item| channel_item.ty == ChannelItemType::Manifest)
		.unwrap()
		.payloads
		.as_ref()
		.unwrap()
		.first()
		.unwrap();
	println!("URL {}", manifest_payload.url);
	println!("SHA256 {}", hex::encode(manifest_payload.sha256));
}

pub fn download_manifest(manifest_url: Url, output_path: PathBuf) {
	let manifest: Manifest = reqwest::blocking::get(manifest_url)
		.unwrap()
		.json()
		.unwrap();
	let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
	std::fs::write(output_path, manifest_bytes).unwrap();
}

pub fn choose_packages(manifest: PathBuf, package_ids: Vec<String>, output_path: PathBuf) {
	// Load the manifest.
	let manifest = std::fs::read(manifest).unwrap();
	let manifest: Manifest = serde_json::from_slice(&manifest).unwrap();
	// Find the payloads for all recursive dependencies of the requested packages.
	let mut package_id_queue = package_ids
		.iter()
		.map(|package_id| package_id.to_owned())
		.collect::<Vec<_>>();
	let mut seen_package_ids = package_ids
		.iter()
		.map(|package_id| package_id.to_owned())
		.collect::<HashSet<_>>();
	let mut packages = Vec::new();
	while let Some(package_id) = package_id_queue.pop() {
		for package in manifest
			.packages
			.iter()
			.filter(|package| package.id.eq_ignore_ascii_case(&package_id))
		{
			packages.push(package);
			for (id, dependency) in package.dependencies.iter() {
				if !seen_package_ids.contains(&id.to_ascii_lowercase()) && dependency.ty.is_none() {
					package_id_queue.push(id.to_owned());
					seen_package_ids.insert(id.to_ascii_lowercase());
				}
			}
		}
	}
	let packages_bytes = serde_json::to_vec_pretty(&packages).unwrap();
	std::fs::write(output_path, &packages_bytes).unwrap();
}

pub fn download_packages(packages_path: PathBuf, cache_path: PathBuf) {
	// Read the packages.
	let packages_bytes = std::fs::read(packages_path).unwrap();
	let packages: Vec<Package> = serde_json::from_slice(&packages_bytes).unwrap();
	// Create the cache path if necessary.
	if !cache_path.exists() {
		std::fs::create_dir_all(&cache_path).unwrap();
	}
	// Download the payloads from all the packages.
	let total_size = packages
		.iter()
		.flat_map(|package| package.payloads.iter())
		.map(|payload| payload.size)
		.sum();
	let progress_bar_style = ProgressStyle::default_bar()
		.template("[{wide_bar}] {bytes} / {total_bytes}")
		.progress_chars("=> ");
	let progress_bar = ProgressBar::new(total_size).with_style(progress_bar_style);
	tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(join_all(packages.into_iter().map(|package| {
			let cache_path = cache_path.clone();
			let progress_bar = progress_bar.clone();
			async move {
				for payload in package.payloads {
					let payload_cache_path = cache_path.join(hex::encode(payload.sha256));
					if payload_cache_path.exists() {
						let bytes = tokio::fs::read(payload_cache_path).await.unwrap();
						progress_bar.inc(payload.size);
						let mut sha256 = Sha256::new();
						sha256.update(&bytes);
						let sha256 = sha256.finalize();
						if sha256.as_slice() != payload.sha256 {
							panic!("hash did not match for cached payload {}", payload.url,);
						}
					} else {
						let mut stream = reqwest::get(payload.url.to_owned())
							.await
							.unwrap()
							.bytes_stream();
						let mut file = tokio::fs::File::create(&payload_cache_path).await.unwrap();
						let mut sha256 = Sha256::new();
						while let Some(chunk) = stream.next().await {
							let chunk = chunk.unwrap();
							let chunk_size = chunk.len() as u64;
							sha256.update(&chunk);
							file.write_all(&chunk).await.unwrap();
							progress_bar.inc(chunk_size);
						}
						let sha256 = sha256.finalize();
						if sha256.as_slice() != payload.sha256 {
							panic!("hash did not match for downloaded payload {}", payload.url,);
						}
					}
				}
			}
		})));
	progress_bar.finish();
}

pub fn extract_packages(packages_path: PathBuf, cache_path: PathBuf, output_path: PathBuf) {
	// Read the packages.
	let packages_bytes = std::fs::read(packages_path).unwrap();
	let packages: Vec<Package> = serde_json::from_slice(&packages_bytes).unwrap();
	// Clean and create the output path.
	if output_path.exists() {
		std::fs::remove_dir_all(&output_path).unwrap();
	}
	std::fs::create_dir_all(&output_path).unwrap();
	let total_size = packages
		.iter()
		.flat_map(|package| package.payloads.iter())
		.map(|payload| payload.size)
		.sum();
	let progress_bar_style = ProgressStyle::default_bar()
		.template("[{wide_bar}] {bytes} / {total_bytes}")
		.progress_chars("=> ");
	let progress_bar = ProgressBar::new(total_size).with_style(progress_bar_style);
	for package in packages {
		let package_tempdir = tempdir().unwrap();
		for payload in package.payloads.iter() {
			let payload_cache_path = cache_path.join(hex::encode(payload.sha256));
			let payload_tempdir_path = package_tempdir
				.path()
				.join(payload.file_name.replace("\\", "/"));
			std::fs::create_dir_all(payload_tempdir_path.parent().unwrap()).unwrap();
			std::fs::copy(payload_cache_path, payload_tempdir_path).unwrap();
		}
		for payload in package.payloads.iter() {
			let payload_tempdir_path = package_tempdir
				.path()
				.join(payload.file_name.replace("\\", "/"));
			enum ExtractionType {
				Msi,
				Vsix,
			}
			let extraction_type = if payload.file_name.ends_with(".msi") {
				Some(ExtractionType::Msi)
			} else if payload.file_name.ends_with(".vsix") {
				Some(ExtractionType::Vsix)
			} else {
				None
			};
			match extraction_type {
				None => {}
				Some(ExtractionType::Msi) => {
					cmd!("msiextract", "-C", &output_path, &payload_tempdir_path)
						.stderr_null()
						.stdout_null()
						.run()
						.unwrap();
				}
				Some(ExtractionType::Vsix) => {
					let unzip_tempdir = tempdir().unwrap();
					cmd!(
						"unzip",
						"-qq",
						&payload_tempdir_path,
						"-d",
						unzip_tempdir.path()
					)
					.read()
					.unwrap();
					if let Ok(contents) = std::fs::read_dir(unzip_tempdir.path().join("Contents")) {
						for entry in contents {
							cmd!("cp", "-r", entry.unwrap().path(), &output_path)
								.run()
								.unwrap();
						}
					}
				}
			}
			progress_bar.inc(payload.size);
		}
	}
	progress_bar.finish();

	// Lowercase all header and import library names.
	let header_paths = || {
		WalkDir::new(&output_path)
			.into_iter()
			.filter_map(|entry| {
				let entry = entry.unwrap();
				let extension = entry.path().extension().map(|e| e.to_str().unwrap());
				match extension {
					Some("h") => Some(entry.path().to_owned()),
					_ => None,
				}
			})
			.collect::<Vec<_>>()
	};
	let import_library_paths = || {
		WalkDir::new(&output_path)
			.into_iter()
			.filter_map(|entry| {
				let entry = entry.unwrap();
				let extension = entry.path().extension().map(|e| e.to_str().unwrap());
				match extension {
					Some("lib") | Some("Lib") => Some(entry.path().to_owned()),
					_ => None,
				}
			})
			.collect::<Vec<_>>()
	};
	header_paths()
		.iter()
		.chain(import_library_paths().iter())
		.for_each(|path| {
			let name = path.file_name().unwrap();
			let lowercase_name = name.to_ascii_lowercase();
			if lowercase_name != name {
				std::fs::rename(&path, path.parent().unwrap().join(lowercase_name)).unwrap();
			}
		});

	// Copy headers to match references with different casing.
	let mut headers = HashMap::new();
	for header_path in header_paths() {
		let file_name = header_path.file_name().unwrap().to_str().unwrap();
		let lowercase_file_name = file_name.to_lowercase();
		let entries = headers
			.entry(lowercase_file_name)
			.or_insert_with(HashSet::new);
		entries.insert(header_path);
	}
	let include_regex = regex::bytes::Regex::new(r#"#include(\s+)(["<])([^">]+)([">])"#).unwrap();
	header_paths().iter().for_each(|header_path| {
		let header_bytes = std::fs::read(header_path).unwrap();
		for capture in include_regex.captures_iter(&header_bytes) {
			let name = std::str::from_utf8(&capture[3]).unwrap();
			if let Some(paths) = headers.get(&name.to_lowercase()) {
				for path in paths {
					let mut path = path.parent().unwrap().to_owned();
					path.push(name);
					if !path.exists() {
						std::fs::write(path, &header_bytes).unwrap();
					}
				}
			}
		}
	});

	// // Lowercase all includes in headers.
	// let include_regex = regex::bytes::Regex::new(r#"#include(\s+)(["<])([^">]+)([">])"#).unwrap();
	// for header_path in header_paths() {
	// 	let header_bytes = std::fs::read(&header_path).unwrap();
	// 	let header_bytes = include_regex.replace_all(&header_bytes, |captures: &regex::bytes::Captures| {
	// 		let mut replacement = b"#include".to_vec();
	// 		replacement.extend(&captures[1]);
	// 		replacement.extend(&captures[2]);
	// 		let name = std::str::from_utf8(&captures[3]).unwrap().to_lowercase();
	// 		replacement.extend(name.as_bytes());
	// 		replacement.extend(&captures[4]);
	// 		replacement
	// 	});
	// 	std::fs::write(&header_path, &header_bytes).unwrap();
	// }
}
