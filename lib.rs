use digest::Digest;
use duct::cmd;
use futures::{future::join_all, StreamExt};
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
	pub dependencies: HashMap<String, Dependency>,
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

pub fn download_manifest(major_version: String, output_path: PathBuf) {
	let channel_url = format!("https://aka.ms/vs/{}/release/channel", major_version);
	let channel: Channel = reqwest::blocking::get(channel_url).unwrap().json().unwrap();
	let manifest_url = channel
		.channel_items
		.iter()
		.find(|channel_item| channel_item.ty == ChannelItemType::Manifest)
		.unwrap()
		.payloads
		.as_ref()
		.unwrap()
		.first()
		.unwrap()
		.url
		.clone();
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
	let mut package_ids = package_ids;
	let mut seen_package_ids = HashSet::new();
	let mut packages = Vec::new();
	while let Some(package_id) = package_ids.pop() {
		if let Some(package) = manifest
			.packages
			.iter()
			.find(|package| package.id == package_id)
		{
			seen_package_ids.insert(package_id.clone());
			packages.push(package);
			for (package_id, dependency) in package.dependencies.iter() {
				if !seen_package_ids.contains(package_id) && dependency.ty.is_none() {
					package_ids.push(package_id.clone());
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
		.template("{msg}\n[{wide_bar}] {bytes} / {total_bytes}")
		.progress_chars("=> ");
	let progress_bar = ProgressBar::new(total_size).with_style(progress_bar_style);
	tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(join_all(packages.into_iter().map(|package| {
			let cache_path = cache_path.clone();
			let progress_bar = progress_bar.clone();
			async move {
				progress_bar.set_message(format!("Downloading {}", package.id));
				let package_cache_path = cache_path.join(&package.id);
				if !package_cache_path.exists() {
					tokio::fs::create_dir_all(&package_cache_path)
						.await
						.unwrap();
				}
				for payload in package.payloads {
					let payload_cache_path =
						package_cache_path.join(&payload.file_name.replace("\\", "/"));
					let mut sha256 = Sha256::new();
					if payload_cache_path.exists() {
						let bytes = tokio::fs::read(payload_cache_path).await.unwrap();
						sha256.update(&bytes);
						progress_bar.inc(payload.size);
					} else {
						let mut stream = reqwest::get(payload.url.to_owned())
							.await
							.unwrap()
							.bytes_stream();
						tokio::fs::create_dir_all(payload_cache_path.parent().unwrap())
							.await
							.unwrap();
						let mut file = tokio::fs::File::create(&payload_cache_path).await.unwrap();
						while let Some(chunk) = stream.next().await {
							let chunk = chunk.unwrap();
							let chunk_size = chunk.len() as u64;
							sha256.update(&chunk);
							file.write_all(&chunk).await.unwrap();
							progress_bar.inc(chunk_size);
						}
					}
					let sha256 = sha256.finalize();
					if sha256.as_slice() != payload.sha256 {
						panic!("hash did not match for {} {}", package.id, payload.url);
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
		.template("{msg}\n[{wide_bar}] {bytes} / {total_bytes}")
		.progress_chars("=> ");
	let progress_bar = ProgressBar::new(total_size).with_style(progress_bar_style);
	for package in packages {
		progress_bar.set_message(format!("Extracting {}", package.id));
		for payload in package.payloads {
			let download_cache_dir_path = cache_path.join(&package.id);
			let download_cache_path =
				download_cache_dir_path.join(&payload.file_name.replace("\\", "/"));
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
					cmd!("msiextract", "-C", &output_path, &download_cache_path)
						.stderr_null()
						.stdout_null()
						.run()
						.unwrap();
				}
				Some(ExtractionType::Vsix) => {
					let tempdir = tempdir().unwrap();
					cmd!("unzip", "-qq", &download_cache_path, "-d", tempdir.path())
						.read()
						.unwrap();
					if let Ok(contents) = std::fs::read_dir(tempdir.path().join("Contents")) {
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
