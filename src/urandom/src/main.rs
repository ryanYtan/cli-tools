use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;
use clap::builder::NonEmptyStringValueParser;

const DEFAULT_CHARSET: &str =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Generate random data
#[derive(Parser)]
struct Args {
    /// Number of characters to generate (bytes with --hex or --bytes)
    #[arg(short = 'i', long, default_value_t = 32)]
    size: usize,

    /// Character set to use
    #[arg(short, long, default_value = DEFAULT_CHARSET, value_parser = NonEmptyStringValueParser::new())]
    charset: String,

    /// Output as HEX
    #[arg(short = 'x', long, conflicts_with = "bytes")]
    hex: bool,

    /// Output as raw bytes
    #[arg(short = 'b', long)]
    bytes: bool,
}

fn random_bytes(buf: &mut [u8]) -> Result<(), String> {
    getrandom::fill(buf).map_err(|e| format!("failed to read random bytes: {e}"))
}

/// Picks `size` characters uniformly from `charset`. Random values past the
/// last whole multiple of the charset's length are redrawn, since reducing
/// them modulo the length would favour the charset's first characters.
fn random_string(charset: &[char], size: usize) -> Result<String, String> {
    let n = charset.len() as u64;
    let limit = (1u64 << 32) / n * n;

    let mut s = String::with_capacity(size);
    let mut picked = 0;
    let mut buf = [0u8; 256];
    while picked < size {
        random_bytes(&mut buf)?;
        for chunk in buf.chunks_exact(4) {
            let v = u64::from(u32::from_le_bytes(chunk.try_into().unwrap()));
            if v >= limit {
                continue;
            }
            s.push(charset[(v % n) as usize]);
            picked += 1;
            if picked == size {
                break;
            }
        }
    }
    Ok(s)
}

fn main() -> ExitCode {
    common::run(env!("CARGO_PKG_NAME"), run)
}

fn run() -> common::Result {
    let args = Args::parse();
    let mut out = io::stdout().lock();

    if args.hex || args.bytes {
        let mut rands = vec![0u8; args.size];
        random_bytes(&mut rands)?;
        if args.hex {
            writeln!(out, "{}", hex_encode(&rands))?;
        } else {
            out.write_all(&rands)?;
        }
    } else {
        let charset: Vec<char> = args.charset.chars().collect();
        writeln!(out, "{}", random_string(&charset, args.size)?)?;
    }
    out.flush()?;
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_string_has_the_requested_length() {
        let charset: Vec<char> = "abc".chars().collect();
        assert_eq!(random_string(&charset, 0).unwrap(), "");
        assert_eq!(random_string(&charset, 1000).unwrap().chars().count(), 1000);
    }

    #[test]
    fn random_string_reaches_every_character_of_a_large_charset() {
        // 300 characters: more than a single random byte could index.
        let charset: Vec<char> = (0..300).map(|i| char::from_u32(0x4e00 + i).unwrap()).collect();
        let s = random_string(&charset, 30_000).unwrap();
        assert!(charset.iter().all(|c| s.contains(*c)));
    }

    #[test]
    fn random_string_draws_each_character_about_equally_often() {
        // 3 doesn't divide 256, so a plain `byte % 3` would skew towards 'a'.
        let charset: Vec<char> = "abc".chars().collect();
        let s = random_string(&charset, 300_000).unwrap();
        for c in &charset {
            let count = s.chars().filter(|x| x == c).count();
            assert!((98_000..102_000).contains(&count), "{c}: {count}");
        }
    }
}
