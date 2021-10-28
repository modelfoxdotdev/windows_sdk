use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[clap(short, long)]
    source: PathBuf,
    #[clap(short, long)]
    destination: PathBuf,
}

fn main() {
    let args = Args::parse();
    if let Err(e) = windows_sdk::run(&args.source, &args.destination) {
        eprintln!("{}", e);
    }
}
