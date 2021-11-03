use regex::bytes::Regex;
use std::{
	collections::{HashMap, HashSet},
	fs, io,
	os::unix::fs::symlink,
	path::{Path, PathBuf},
};
use walkdir::WalkDir;

/// Build the `toplevel/{clang,crt,sdk}/{lib,include}/` structure
fn build_structure(toplevel: impl AsRef<Path>) -> io::Result<()> {
	let toplevel = toplevel.as_ref();
	if toplevel.exists() {
		fs::remove_dir_all(toplevel)?;
	}
	fs::create_dir(toplevel)?;
	let top_levels = ["clang", "crt", "sdk"];
	for dir in top_levels {
		let inner_levels = ["lib", "include"];
		for inner in inner_levels {
			let d = toplevel.join(dir).join(inner);
			fs::create_dir_all(&d)?;
		}
	}
	Ok(())
}

pub fn copy_dir_all(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> io::Result<()> {
	let mut stack = Vec::new();
	stack.push(PathBuf::from(source.as_ref()));

	let output_root = PathBuf::from(destination.as_ref());
	let input_root = PathBuf::from(source.as_ref()).components().count();

	while let Some(working_path) = stack.pop() {
		// Generate a relative path
		let src: PathBuf = working_path.components().skip(input_root).collect();

		// Create a destination if missing
		let dest = if src.components().count() == 0 {
			output_root.clone()
		} else {
			output_root.join(&src)
		};
		if fs::metadata(&dest).is_err() {
			fs::create_dir_all(&dest)?;
		}

		for entry in fs::read_dir(working_path)? {
			let entry = entry?;
			let path = entry.path();
			if path.is_dir() {
				stack.push(path);
			} else {
				match path.file_name() {
					Some(filename) => {
						let dest_path = dest.join(filename);
						fs::copy(&path, &dest_path)?;
					}
					None => { /* nothing to do! */ }
				}
			}
		}
	}

	Ok(())
}

/// Copy the necessary folders from the source SDK directory tree to our tailored destination.
fn copy_contents(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> io::Result<()> {
	// Set up reused paths
	let source = source.as_ref();
	let destination = destination.as_ref();
	let sdk_ver = "10.0.19041.0";
	let vc_tools_ver = "14.29.30133";
	let llvm_version = "12.0.0";

	// Source locations
	let sdk_path_src = source.join("kits").join("10");
	let sdk_includes_src = sdk_path_src.join("Include").join(sdk_ver);
	let sdk_shared_src = sdk_includes_src.join("shared");
	let sdk_ucrt_src = sdk_includes_src.join("ucrt");
	let sdk_um_src = sdk_includes_src.join("um");
	let sdk_libs_src = sdk_path_src.join("Lib").join(sdk_ver);
	let sdk_x64_libs_fn = |subdir: &str| sdk_libs_src.join(subdir).join("x64");
	let sdk_ucrt_libs_src = sdk_x64_libs_fn("ucrt");
	let sdk_um_libs_src = sdk_x64_libs_fn("um");
	let vc_tools_path_src = source.join("VC").join("Tools");
	let vc_tools_includes_src = vc_tools_path_src
		.join("MSVC")
		.join(vc_tools_ver)
		.join("include");
	let vc_tools_clang_compat_src = vc_tools_path_src
		.join("Llvm")
		.join("x64")
		.join("lib")
		.join("clang")
		.join(llvm_version)
		.join("include");
	let vc_tools_libs_src = vc_tools_path_src
		.join("MSVC")
		.join(vc_tools_ver)
		.join("lib")
		.join("x64");

	// Destination locations
	let sdk_includes_dst = destination.join("sdk").join("include");
	let sdk_shared_dst = sdk_includes_dst.join("shared");
	let sdk_ucrt_dst = sdk_includes_dst.join("ucrt");
	let sdk_um_dst = sdk_includes_dst.join("um");
	let sdk_libs_fn = |subdir: &str| destination.join("sdk").join("lib").join("x64").join(subdir);
	let sdk_ucrt_libs_dst = sdk_libs_fn("ucrt");
	let sdk_um_libs_dst = sdk_libs_fn("um");
	let crt_dst = destination.join("crt");
	let crt_includes_dst = crt_dst.join("include");
	let crt_clang_compat_dst = destination.join("clang").join("include");
	let crt_libs_dst = crt_dst.join("lib").join("x64");

	let tasks = [
		(&sdk_shared_src, &sdk_shared_dst),
		(&sdk_ucrt_libs_src, &sdk_ucrt_libs_dst),
		(&sdk_ucrt_src, &sdk_ucrt_dst),
		(&sdk_um_libs_src, &sdk_um_libs_dst),
		(&sdk_um_src, &sdk_um_dst),
		(&vc_tools_clang_compat_src, &crt_clang_compat_dst),
		(&vc_tools_includes_src, &crt_includes_dst),
		(&vc_tools_libs_src, &crt_libs_dst),
	];
	for (source, target) in tasks {
		copy_dir_all(source, target)?;
	}

	Ok(())
}

/// Crawl through every include dir, add every single header to a big map.
fn read_all_headers(toplevel: impl AsRef<Path>) -> io::Result<HashMap<String, PathBuf>> {
	let toplevel = toplevel.as_ref();
	let mut headers = HashMap::new();

	for entry in WalkDir::new(toplevel) {
		let entry = entry?;
		let ftype = entry.file_type();
		if ftype.is_file() {
			let file_name = entry.file_name();
			let file_name = file_name.to_str().unwrap().to_owned();
			let ext = file_name.split('.').nth(1).unwrap_or("_");
			if ext == "h" {
				headers.insert(file_name, entry.path().to_owned());
			}
		}
	}

	Ok(headers)
}

/// Create a symlink to any non-lowercase filename in the path
fn create_lowercase_symlinks(toplevel: impl AsRef<Path>) -> io::Result<()> {
	let toplevel = toplevel.as_ref();
	for entry in WalkDir::new(toplevel) {
		let entry = entry?;
		let ftype = entry.file_type();
		if ftype.is_file() {
			let orig = entry.file_name();
			let orig = orig.to_str().expect("Filename contained invalid UTF-8");
			let lowered = orig.to_lowercase();
			if orig != lowered {
				let source = fs::canonicalize(entry.path())?;
				let mut target = source.clone();
				target.pop();
				target.push(lowered);
				symlink(&source, &target)?;
			}
		}
	}
	Ok(())
}

/// Search files for includes with case issues, create any missing symlinks.
fn symlink_case_mismatches(
	toplevel: impl AsRef<Path>,
	headers: HashMap<String, PathBuf>,
) -> io::Result<()> {
	let toplevel = toplevel.as_ref();

	let sdk = toplevel.join("sdk");
	for subdir in &[sdk.join("include").join("um"), sdk.join("lib")] {
		create_lowercase_symlinks(subdir)?;
	}

	let regex = Regex::new(r#"#include\s+(?:"|<)([^">]+)(?:"|>)?"#).unwrap();

	let mut expected = HashSet::new();
	for entry in WalkDir::new(toplevel) {
		let entry = entry?;
		let ftype = entry.file_type();
		if ftype.is_file() {
			let contents = fs::read(entry.path())?;
			for caps in regex.captures_iter(&contents) {
				let name =
					std::str::from_utf8(&caps[1]).expect("Include contains non-utf8 characters");
				let name = match name.rfind('/') {
					Some(i) => &name[i + 1..],
					None => name,
				};
				expected.insert(name.to_owned());
			}
		}
	}

	for name in expected {
		match headers.get(&name) {
			Some(_) => { /* nothing to do! */ }
			None => {
				// is the base name all lowercase?
				if name
					.split('.')
					.next()
					.unwrap()
					.chars()
					.all(char::is_lowercase)
				{
					// search headers for the mixed-case version, build symlink to expected.
					for (possible_header, path) in &headers {
						let lowered = possible_header.to_lowercase();
						if lowered == name {
							let source = fs::canonicalize(path)?;
							let mut target = source.clone();
							target.pop();
							target.push(name);
							if !target.exists() {
								symlink(source, target)?;
							}
							break;
						}
					}
				} else {
					//the headers should have a lowercase verison.  Build symlink to expected.
					let lowered = name.to_lowercase();
					if let Some(needle) = headers.get(&lowered) {
						let source = fs::canonicalize(needle)?;
						let mut target = source.clone();
						target.pop();
						target.push(name);
						if !target.exists() {
							symlink(source, target)?;
						}
					}
				}
			}
		}
	}
	Ok(())
}

/// Build a tailored MSVC SDK from the full downloaded tree.
pub fn run(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> io::Result<()> {
	println!("Processing sdk...");
	let source = source.as_ref();
	let destination = destination.as_ref();
	build_structure(destination)?;
	copy_contents(source, destination)?;
	symlink_case_mismatches(destination, read_all_headers(destination)?)?;
	println!("All done!");
	Ok(())
}
