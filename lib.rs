use duct::cmd;
use futures::{future::join_all, StreamExt};
use rayon::prelude::*;
use std::{
	collections::{HashMap, HashSet},
	path::PathBuf,
};
use tempfile::tempdir;
use tokio::io::AsyncWriteExt;
use url::Url;
use walkdir::WalkDir;

#[derive(Debug, serde::Deserialize)]
pub struct Manifest {
	#[serde(rename = "manifestVersion")]
	pub manifest_version: String,
	#[serde(rename = "engineVersion")]
	pub engine_version: String,
	pub packages: Vec<Package>,
}

#[derive(Debug, serde::Deserialize)]
pub struct Package {
	pub id: String,
	pub version: String,
	#[serde(rename = "type")]
	pub ty: Type,
	#[serde(default)]
	pub dependencies: HashMap<String, Dependency>,
	#[serde(default)]
	pub payloads: Vec<Payload>,
}

#[derive(Debug, serde::Deserialize)]
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

#[derive(Debug, serde::Deserialize)]
pub enum DependencyType {
	Optional,
	Recommended,
}

#[derive(Debug, serde::Deserialize)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
pub enum Type {
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

#[derive(Debug, serde::Deserialize)]
pub struct Payload {
	#[serde(rename = "fileName")]
	pub file_name: String,
	pub sha256: String,
	pub size: u64,
	pub url: Url,
}

pub fn build(
	manifest_url: Url,
	packages: Vec<String>,
	cache_path: PathBuf,
	output_path: PathBuf,
) {
	// Create the cache path if necessary.
	if !cache_path.exists() {
		std::fs::create_dir_all(&output_path).unwrap();
	}
	// Create a tempdir to extract to.
	let extract_dir = tempdir().unwrap();
	let extract_path = extract_dir.path();
	// Clean and create the output path.
	if output_path.exists() {
		std::fs::remove_dir_all(&output_path).unwrap();
	}
	std::fs::create_dir_all(&output_path).unwrap();
	eprintln!("Downloading manifest");

	// Load the manifest.
	let manifest = reqwest::blocking::get(manifest_url)
		.unwrap()
		.bytes()
		.unwrap();
	let manifest: Manifest = serde_json::from_slice(&manifest).unwrap();

	// Find the payloads for all recursive dependencies of the requested components.
	#[derive(Debug)]
	struct Download {
		package_id: String,
		file_name: String,
		url: Url,
	}
	#[derive(Debug)]
	struct Extraction {
		package_id: String,
		file_name: String,
		ty: ExtractionType,
	}
	#[derive(Debug)]
	enum ExtractionType {
		Msi,
		Vsix,
	}
	let mut packages = packages;
	let mut seen_packages = HashSet::new();
	let mut downloads = Vec::new();
	let mut extractions = Vec::new();
	while let Some(package_id) = packages.pop() {
		if let Some(package) = manifest
			.packages
			.iter()
			.find(|package| package.id == package_id)
		{
			seen_packages.insert(package_id.clone());
			for payload in package.payloads.iter() {
				let file_name = payload.file_name.replace("\\", "/");
				downloads.push(Download {
					package_id: package_id.clone(),
					file_name: file_name.clone(),
					url: payload.url.clone(),
				});
				let extraction_ty = if payload.file_name.ends_with(".msi") {
					Some(ExtractionType::Msi)
				} else if payload.file_name.ends_with(".vsix") {
					Some(ExtractionType::Vsix)
				} else {
					None
				};
				if let Some(ty) = extraction_ty {
					extractions.push(Extraction {
						package_id: package_id.clone(),
						file_name,
						ty,
					})
				}
			}
			for (package_id, dependency) in package.dependencies.iter() {
				if !seen_packages.contains(package_id) && dependency.ty.is_none() {
					packages.push(package_id.clone());
				}
			}
		}
	}

	// Download.
	let n = downloads.len();
	tokio::runtime::Runtime::new().unwrap().block_on(join_all(
		downloads.into_iter().enumerate().map(|(i, download)| {
			let cache_path = cache_path.clone();
			async move {
				let download_cache_dir_path = cache_path.join(&download.package_id);
				if !download_cache_dir_path.exists() {
					tokio::fs::create_dir_all(&download_cache_dir_path)
						.await
						.unwrap();
				}
				let download_cache_path = download_cache_dir_path.join(&download.file_name);
				if !download_cache_path.exists() {
					eprintln!(
						"({} / {}) Downloading {} {}",
						i + 1,
						n,
						download.package_id,
						download.file_name,
					);
					let mut stream = reqwest::get(download.url.to_owned())
						.await
						.unwrap()
						.bytes_stream();
					tokio::fs::create_dir_all(download_cache_path.parent().unwrap())
						.await
						.unwrap();
					let mut file = tokio::fs::File::create(&download_cache_path).await.unwrap();
					while let Some(chunk) = stream.next().await {
						file.write_all(&chunk.unwrap()).await.unwrap();
					}
				} else {
					eprintln!(
						"({} / {}) Cached {} {}",
						i + 1,
						n,
						download.package_id,
						download.file_name,
					);
				}
			}
		}),
	));

	// Extract.
	extractions
		.par_iter()
		.enumerate()
		.for_each(|(i, extraction)| {
			let download_cache_dir_path = cache_path.join(&extraction.package_id);
			let download_cache_path = download_cache_dir_path.join(&extraction.file_name);
			eprintln!(
				"({} / {}) Extracting {}",
				i + 1,
				extractions.len(),
				extraction.file_name,
			);
			match extraction.ty {
				ExtractionType::Msi => {
					cmd!("msiextract", "-C", &extract_path, &download_cache_path)
						.run()
						.unwrap();
				}
				ExtractionType::Vsix => {
					let tempdir = tempdir().unwrap();
					cmd!("unzip", "-qq", &download_cache_path, "-d", tempdir.path())
						.run()
						.unwrap();
					if let Ok(contents) = std::fs::read_dir(tempdir.path().join("Contents")) {
						for entry in contents {
							cmd!("cp", "-r", entry.unwrap().path(), &extract_path)
								.run()
								.unwrap();
						}
					}
				}
			}
		});

	// Lowercase all header and import library names.
	let header_paths = || {
		WalkDir::new(&extract_path)
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
		WalkDir::new(&extract_path)
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

	// Copy the result to the output path.
	if extract_path
		.join("Program Files")
		.join("Windows Kits")
		.exists()
	{
		cmd!(
			"cp",
			"-r",
			extract_path.join("Program Files").join("Windows Kits"),
			&output_path,
		)
		.run()
		.unwrap();
	}
	if extract_path.join("VC").join("Tools").exists() {
		std::fs::create_dir_all(output_path.join("VC")).unwrap();
		cmd!(
			"cp",
			"-r",
			extract_path.join("VC").join("Tools"),
			output_path.join("VC"),
		)
		.run()
		.unwrap();
	}
}
