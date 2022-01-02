use clap::Parser;
use std::path::PathBuf;
use url::Url;

#[derive(Parser)]
struct Args {
	#[clap(long)]
	manifest_url: Url,
	#[clap(long = "package", value_name = "PACKAGE", required = true)]
	packages: Vec<String>,
	#[clap(long)]
	cache: PathBuf,
	#[clap(long)]
	output: PathBuf,
}

fn main() {
	let args = Args::parse();
	windows_sdk::build(args.manifest_url, args.packages, args.cache, args.output);
}
