use std::io::Write;

use clap::Parser;

const DEFAULT_CHARSET: &str =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Generate random data
#[derive(Parser)]
#[command(about = "Generate random data")]
struct Args {
    /// Number of bytes to generate
    #[arg(short = 'i', long, default_value_t = 32)]
    size: usize,

    /// Character set to use
    #[arg(short, long, default_value = DEFAULT_CHARSET)]
    charset: String,

    /// Output as HEX
    #[arg(short = 'x', long, conflicts_with = "bytes")]
    hex: bool,

    /// Output as a bytes
    #[arg(short = 'b', long)]
    bytes: bool,
}

fn main() {
    let args = Args::parse();

    let mut rands = vec![0u8; args.size];
    getrandom::fill(&mut rands).expect("failed to read random bytes");

    if args.hex {
        println!("{}", hex_encode(&rands));
    } else if args.bytes {
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        lock.write_all(&rands).expect("failed to write to stdout");
        lock.flush().expect("failed to flush stdout");
    } else {
        let charset: Vec<char> = args.charset.chars().collect();
        let s: String = rands
            .iter()
            .map(|b| charset[(*b as usize) % charset.len()])
            .collect();
        println!("{s}");
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
