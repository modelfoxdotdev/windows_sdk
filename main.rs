use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[clap(
	version = concat!(env!("CARGO_PKG_VERSION")),
	setting = clap::AppSettings::DisableHelpSubcommand,
)]
struct Args {
	#[clap(subcommand)]
	subcommand: Subcommand,
}

#[derive(Parser)]
enum Subcommand {
	#[clap(name = "download-manifest")]
	DownloadManifest(DownloadManifestArgs),
	#[clap(name = "choose-packages")]
	ChoosePackages(ChoosePackagesArgs),
	#[clap(name = "download-packages")]
	DownloadPackages(DownloadPackagesArgs),
	#[clap(name = "extract-packages")]
	ExtractPackages(ExtractPackagesArgs),
}

#[derive(Parser)]
struct DownloadManifestArgs {
	#[clap(long)]
	major_version: String,
	#[clap(long)]
	output: PathBuf,
}

#[derive(Parser)]
struct ChoosePackagesArgs {
	#[clap(long)]
	manifest: PathBuf,
	#[clap(long = "package", value_name = "PACKAGE", required = true)]
	packages: Vec<String>,
	#[clap(long)]
	output: PathBuf,
}

#[derive(Parser)]
struct DownloadPackagesArgs {
	#[clap(long)]
	packages: PathBuf,
	#[clap(long)]
	cache: PathBuf,
}

#[derive(Parser)]
struct ExtractPackagesArgs {
	#[clap(long)]
	packages: PathBuf,
	#[clap(long)]
	cache: PathBuf,
	#[clap(long)]
	output: PathBuf,
}

fn main() {
	let args = Args::parse();
	match args.subcommand {
		Subcommand::DownloadManifest(args) => {
			windows_sdk::download_manifest(args.major_version, args.output);
		}
		Subcommand::ChoosePackages(args) => {
			windows_sdk::choose_packages(args.manifest, args.packages, args.output);
		}
		Subcommand::DownloadPackages(args) => {
			windows_sdk::download_packages(args.packages, args.cache);
		}
		Subcommand::ExtractPackages(args) => {
			windows_sdk::extract_packages(args.packages, args.cache, args.output);
		}
	}
}
