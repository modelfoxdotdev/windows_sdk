use clap::Parser;
use std::path::PathBuf;
use url::Url;

#[derive(Parser)]
struct Args {
	#[clap(long)]
	manifest_url: Url,
	#[clap(long)]
	package_ids: Vec<String>,
	#[clap(long)]
	cache: PathBuf,
	#[clap(long)]
	output: PathBuf,
}

fn main() {
	let args = Args::parse();
	windows_sdk::build(args.manifest_url, args.package_ids, args.cache, args.output);
}
