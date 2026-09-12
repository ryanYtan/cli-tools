use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;

/// Prints the size of a file, or the total size of a directory's contents
#[derive(Parser)]
#[command(
    about = "Prints the size of a file, or the total size of a directory's contents",
    disable_help_flag = true
)]
struct Args {
    /// Path to measure
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Print size in bytes (default)
    #[arg(short, long, group = "unit")]
    bytes: bool,

    /// Print size in kibibytes (1024 bytes)
    #[arg(short, long, group = "unit")]
    kilo: bool,

    /// Print size in mebibytes (1024^2 bytes)
    #[arg(short, long, group = "unit")]
    mega: bool,

    /// Print size in gibibytes (1024^3 bytes)
    #[arg(short, long, group = "unit")]
    giga: bool,

    /// Print size in tebibytes (1024^4 bytes)
    #[arg(short, long, group = "unit")]
    tera: bool,

    /// Automatically scale to the largest unit that keeps the value readable
    #[arg(short = 'h', long = "human-readable", group = "unit")]
    human_readable: bool,

    /// Print help
    #[arg(long = "help", action = clap::ArgAction::Help)]
    help: Option<bool>,
}

fn size_of(path: &Path) -> std::io::Result<u64> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() {
        return Ok(meta.len());
    }

    let mut total = 0u64;
    for entry in fs::read_dir(path)? {
        total += size_of(&entry?.path())?;
    }
    Ok(total)
}

fn format_fixed(bytes: u64, unit_size: u64, suffix: &str) -> String {
    format!("{:.1}{suffix}", bytes as f64 / unit_size as f64)
}

fn format_human(bytes: u64) -> String {
    const UNITS: [(u64, &str); 4] = [
        (1u64 << 40, "T"),
        (1u64 << 30, "G"),
        (1u64 << 20, "M"),
        (1u64 << 10, "K"),
    ];
    for (unit_size, suffix) in UNITS {
        if bytes >= unit_size {
            return format_fixed(bytes, unit_size, suffix);
        }
    }
    bytes.to_string()
}

fn main() {
    let args = Args::parse();

    let bytes = size_of(&args.path).unwrap_or_else(|e| {
        eprintln!("failed to read size of {}: {e}", args.path.display());
        std::process::exit(1);
    });

    let output = if args.kilo {
        format_fixed(bytes, 1 << 10, "K")
    } else if args.mega {
        format_fixed(bytes, 1 << 20, "M")
    } else if args.giga {
        format_fixed(bytes, 1 << 30, "G")
    } else if args.tera {
        format_fixed(bytes, 1 << 40, "T")
    } else if args.human_readable {
        format_human(bytes)
    } else {
        bytes.to_string()
    };

    println!("{output}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn format_fixed_divides_and_appends_the_suffix() {
        assert_eq!(format_fixed(2048, 1024, "K"), "2.0K");
        assert_eq!(format_fixed(1536, 1024, "K"), "1.5K");
    }

    #[test]
    fn format_human_picks_the_largest_fitting_unit() {
        assert_eq!(format_human(512), "512");
        assert_eq!(format_human(2048), "2.0K");
        assert_eq!(format_human(1 << 20), "1.0M");
        assert_eq!(format_human(3 * (1u64 << 30)), "3.0G");
        assert_eq!(format_human((1u64 << 40) + (1u64 << 39)), "1.5T");
    }

    #[test]
    fn size_of_a_file_is_its_byte_length() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(&[0u8; 1234]).unwrap();
        assert_eq!(size_of(f.path()).unwrap(), 1234);
    }

    #[test]
    fn size_of_a_directory_sums_its_contents_recursively() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), [0u8; 100]).unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("b.txt"), [0u8; 250]).unwrap();
        assert_eq!(size_of(dir.path()).unwrap(), 350);
    }

    #[test]
    fn size_of_an_empty_directory_is_zero() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(size_of(dir.path()).unwrap(), 0);
    }

    #[test]
    fn size_of_a_missing_path_is_an_error() {
        assert!(size_of(Path::new("/no/such/path/at/all")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn size_of_does_not_follow_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("big.txt");
        std::fs::write(&target, [0u8; 5000]).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        // The symlink's own size, not the 5000-byte target it points to.
        assert!(size_of(&link).unwrap() < 5000);
    }
}
