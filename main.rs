use clap::Parser;
use std::path::PathBuf;
use tracing::error;

#[derive(Parser)]
struct Args {
    #[clap(short, long)]
    source: PathBuf,
    #[clap(short, long)]
    destination: PathBuf,
}

fn main() {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    if let Err(e) = windows_sdk::run(&args.source, &args.destination) {
        error!("{}", e);
    }
}
