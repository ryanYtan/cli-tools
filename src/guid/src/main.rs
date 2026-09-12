use clap::Parser;
use uuid::Uuid;

/// Generates GUIDs (v4)
#[derive(Parser)]
#[command(about = "Generates GUIDs (v4)")]
struct Args {
    /// Number of GUIDs to generate
    #[arg(short, long, default_value_t = 1)]
    count: u32,

    /// Generate a GUID from a 128-bit integer
    #[arg(short = 'i', long, conflicts_with = "empty")]
    from_integer: Option<u128>,

    /// Generate an empty GUID (all bits 0)
    #[arg(short, long)]
    empty: bool,

    /// Use uppercase letters
    #[arg(short, long, conflicts_with = "lowercase")]
    uppercase: bool,

    /// Use lowercase letters
    #[arg(short, long)]
    lowercase: bool,
}

fn main() {
    let args = Args::parse();

    for _ in 0..args.count {
        let guid = if let Some(from_integer) = args.from_integer {
            Uuid::from_u128(from_integer)
        } else if args.empty {
            Uuid::nil()
        } else {
            Uuid::new_v4()
        };

        if args.uppercase {
            println!("{}", guid.hyphenated().to_string().to_uppercase());
        } else {
            println!("{guid}");
        }
    }
}
