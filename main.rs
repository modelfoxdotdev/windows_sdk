use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[clap(short, long)]
    source: PathBuf,
    #[clap(short, long)]
    destination: PathBuf,
    #[clap(short, long)]
    use_std: bool,
}

fn main() {
    let args = Args::parse();
    if let Err(e) = windows_sdk::run(&args.source, &args.destination, args.use_std) {
        eprintln!("{}", e);
    }
}
