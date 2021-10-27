use regex::bytes::Regex;
use std::{
    collections::{HashMap, HashSet},
    fs, io,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

/// Build the `toplevel/{crt,sdk}/{lib,include}/` structure, with some extras in the `crt` path.
fn build_structure(toplevel: impl AsRef<Path>) -> io::Result<()> {
    let toplevel = toplevel.as_ref();
    if toplevel.exists() {
        fs::remove_dir_all(toplevel)?;
    }
    fs::create_dir(toplevel)?;
    let top_levels = ["crt", "sdk"];
    for dir in top_levels {
        let inner_levels = ["lib", "include"];
        for inner in inner_levels {
            let d = toplevel.join(dir).join(inner);
            fs::create_dir_all(&d)?;
            // TODO - do we need to do this here?  copy_contents() might be able to handle it
            if dir == "crt" {
                if inner == "lib" {
                    let p = d.join("x64");
                    fs::create_dir(p)?;
                } else if inner == "include" {
                    let p = d.join("clang");
                    fs::create_dir(p)?;
                }
            }
        }
    }
    Ok(())
}

/// Recursively copy a directory and all contents, like `cp -r`.
fn copy_dir_all(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> io::Result<()> {
    let source = source.as_ref();
    let destination = destination.as_ref();
    fs::create_dir_all(destination)?;
    for entry in WalkDir::new(source) {
        let entry = entry?;
        let ftype = entry.file_type();
        if ftype.is_file() {
            fs::copy(entry.path(), destination.join(entry.file_name()))?;
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
    // Source locations
    let sdk_path = source.join("kits").join("10");
    let sdk_includes = sdk_path.join("Include").join(sdk_ver);
    let sdk_shared = sdk_includes.join("shared");
    let sdk_ucrt = sdk_includes.join("ucrt");
    let sdk_um = sdk_includes.join("um");
    let sdk_libs = sdk_path.join("Lib").join(sdk_ver);
    let sdk_x64_libs = |subdir: &str| sdk_libs.join(subdir).join("x64");
    let sdk_ucrt_libs = sdk_x64_libs("ucrt");
    let sdk_um_libs = sdk_x64_libs("um");
    let vc_tools_path = source.join("VC").join("Tools");
    let vc_tools_includes = vc_tools_path
        .join("MSVC")
        .join(vc_tools_ver)
        .join("include");
    let vc_tools_clang_compat = vc_tools_path
        .join("Llvm")
        .join("x64")
        .join("lib")
        .join("clang")
        .join("12.0.0")
        .join("include");
    let vc_tools_libs = vc_tools_path
        .join("MSVC")
        .join(vc_tools_ver)
        .join("lib")
        .join("x64");
    // Destination locations
    let sdk_includes_dest = destination.join("sdk").join("include");
    let sdk_shared_dest = sdk_includes_dest.join("shared");
    let sdk_ucrt_dest = sdk_includes_dest.join("ucrt");
    let sdk_um_dest = sdk_includes_dest.join("um");
    let sdk_libs_dest = |subdir: &str| destination.join("sdk").join("lib").join(subdir).join("x64");
    let sdk_ucrt_libs_dest = sdk_libs_dest("ucrt");
    let sdk_um_libs_dest = sdk_libs_dest("um");
    let crt = destination.join("crt");
    let crt_includes = crt.join("include");
    let crt_clang_compat = crt_includes.join("clang");
    let crt_libs_dest = crt.join("lib").join("x64");

    let tasks = [
        (&sdk_shared, &sdk_shared_dest),
        (&sdk_ucrt, &sdk_ucrt_dest),
        (&sdk_um, &sdk_um_dest),
        (&vc_tools_includes, &crt_includes),
        (&vc_tools_clang_compat, &crt_clang_compat),
        (&sdk_ucrt_libs, &sdk_ucrt_libs_dest),
        (&sdk_um_libs, &sdk_um_libs_dest),
        (&vc_tools_libs, &crt_libs_dest),
    ];
    for (source, target) in tasks {
        copy_dir_all(source, target)?;
    }

    Ok(())
}

/// Create a symlink from source to target, emitting a message when RUST_LOG=debug is set.
#[inline]
fn debug_symlink(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> io::Result<()> {
    let source = source.as_ref();
    let destination = destination.as_ref();
    symlink(source, destination)?;
    Ok(())
}

// Crawl through every include dir, add every single header to a big map.
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

/// Search files for includes with case issues, create any missing symlinks.
fn create_included_header_symlinks(
    toplevel: impl AsRef<Path>,
    headers: HashMap<String, PathBuf>,
) -> io::Result<()> {
    let toplevel = toplevel.as_ref();

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
                            debug_symlink(source, target)?;
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
                        debug_symlink(source, target)?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Resolve case issues by symlinking.
fn symlink_case_mismatches(
    toplevel: impl AsRef<Path>,
    headers: HashMap<String, PathBuf>,
) -> io::Result<()> {
    let toplevel = toplevel.as_ref();
    create_included_header_symlinks(toplevel, headers)?;
    Ok(())
}

/// Build a tailored MSVC SDK from the full downloaded tree.
pub fn run(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> io::Result<()> {
    println!("Processing sdk...");
    let source = source.as_ref();
    let destination = destination.as_ref();
    build_structure(destination)?;
    copy_contents(source, destination)?;
    let headers = read_all_headers(destination)?;
    symlink_case_mismatches(destination, headers)?;
    println!("All done!");
    Ok(())
}
