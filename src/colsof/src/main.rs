use std::collections::BTreeSet;
use std::io::{self, Write};

use calamine::{open_workbook_auto, Data, DataType, Reader};
use clap::Parser;

/// Prints the columns of an Excel sheet
#[derive(Parser)]
#[command(about = "Prints the columns of an Excel sheet")]
struct Args {
    /// Excel file to read
    file: String,

    /// Name or index of the sheet to read
    #[arg(short = 'i', long, required = true)]
    sheet_name: String,

    /// Skip the first row of the sheet (default)
    #[arg(short = 'w', long, overrides_with = "no_skip_header")]
    skip_header: bool,

    /// Print the first row of the sheet too
    #[arg(short = 'W', long, overrides_with = "skip_header")]
    no_skip_header: bool,

    /// Delimiter to use between columns
    #[arg(short, long, default_value = " ")]
    delimiter: String,

    /// Trim whitespace from columns
    #[arg(short, long)]
    trim: bool,

    /// Column to read (either as index or letter)
    #[arg(short, long, num_args = 1.., default_value = "0", value_parser = col_to_index)]
    columns: Vec<u32>,

    /// Default value to use for empty cells
    #[arg(short = 'f', long, default_value = "<EMPTY>")]
    default_value: String,
}

fn col_to_index(col: &str) -> Result<u32, String> {
    let out_of_range = || format!("column {col} is out of range");
    if !col.is_empty() && col.chars().all(|c| c.is_ascii_digit()) {
        return col.parse().map_err(|_| out_of_range());
    }
    if col.is_empty() || !col.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(format!("{col:?} is not a column index or letter"));
    }
    let mut index: u32 = 0;
    for c in col.to_ascii_uppercase().chars() {
        index = index
            .checked_mul(26)
            .and_then(|i| i.checked_add(c as u32 - 'A' as u32 + 1))
            .ok_or_else(out_of_range)?;
    }
    Ok(index)
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Data::DateTime(dt) => match dt.as_datetime() {
            Some(naive) => naive.to_string(),
            None => cell.to_string(),
        },
        _ => cell.to_string(),
    }
}

fn cell_is_truthy(cell: &Data) -> bool {
    match cell {
        Data::Empty => false,
        Data::Int(i) => *i != 0,
        Data::Float(f) => *f != 0.0,
        Data::Bool(b) => *b,
        Data::String(s) => !s.is_empty(),
        Data::DateTimeIso(s) => !s.is_empty(),
        Data::DurationIso(s) => !s.is_empty(),
        Data::DateTime(_) | Data::Error(_) => true,
    }
}

/// Exits quietly when stdout is closed early (e.g. `colsof | head`).
fn write_failed(e: io::Error) -> ! {
    if e.kind() == io::ErrorKind::BrokenPipe {
        std::process::exit(0);
    }
    eprintln!("failed to write to stdout: {e}");
    std::process::exit(1);
}

fn main() {
    let args = Args::parse();

    let allowed_columns: BTreeSet<u32> = args.columns.iter().copied().collect();

    let mut workbook =
        open_workbook_auto(&args.file).unwrap_or_else(|e| panic!("failed to open {}: {e}", args.file));

    let range = if args.sheet_name.chars().all(|c| c.is_ascii_digit()) && !args.sheet_name.is_empty()
    {
        let sheet_index: usize = args.sheet_name.parse().unwrap();
        workbook
            .worksheet_range_at(sheet_index)
            .unwrap_or_else(|| panic!("no sheet at index {sheet_index}"))
            .unwrap_or_else(|e| panic!("failed to read sheet {sheet_index}: {e}"))
    } else {
        workbook
            .worksheet_range(&args.sheet_name)
            .unwrap_or_else(|e| panic!("failed to read sheet {}: {e}", args.sheet_name))
    };

    let Some((start_row_idx, start_col_idx)) = range.start() else {
        return;
    };
    let _ = start_row_idx;

    let mut rows = range.rows();
    if args.skip_header || !args.no_skip_header {
        rows.next();
    }

    let mut final_columns: BTreeSet<u32> = BTreeSet::new();
    let mut first = true;
    let mut values: Vec<String> = Vec::new();
    let mut out = io::stdout().lock();

    for row in rows {
        values.clear();
        for (j, cell) in row.iter().enumerate() {
            let col = start_col_idx + j as u32 + 1;

            if first {
                if !allowed_columns.contains(&col) || cell.is_empty() {
                    continue;
                }
                final_columns.insert(col);
            } else if !final_columns.contains(&col) {
                continue;
            }

            if cell_is_truthy(cell) {
                let s = cell_to_string(cell);
                values.push(if args.trim { s.trim().to_string() } else { s });
            } else {
                values.push(args.default_value.clone());
            }
        }
        writeln!(out, "{}", values.join(&args.delimiter)).unwrap_or_else(|e| write_failed(e));
        first = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn col_to_index_parses_plain_numbers_as_is() {
        assert_eq!(col_to_index("0"), Ok(0));
        assert_eq!(col_to_index("5"), Ok(5));
    }

    #[test]
    fn col_to_index_converts_single_letters() {
        assert_eq!(col_to_index("A"), Ok(1));
        assert_eq!(col_to_index("a"), Ok(1));
        assert_eq!(col_to_index("Z"), Ok(26));
    }

    #[test]
    fn col_to_index_converts_multi_letter_columns() {
        assert_eq!(col_to_index("AA"), Ok(27));
        assert_eq!(col_to_index("AZ"), Ok(52));
        assert_eq!(col_to_index("BA"), Ok(53));
    }

    #[test]
    fn col_to_index_rejects_anything_else() {
        assert!(col_to_index("").is_err());
        assert!(col_to_index("-").is_err());
        assert!(col_to_index("A1").is_err());
        assert!(col_to_index("é").is_err());
        assert!(col_to_index("99999999999").is_err());
        assert!(col_to_index("ZZZZZZZZZZ").is_err());
    }

    #[test]
    fn cell_to_string_formats_bools_like_python() {
        assert_eq!(cell_to_string(&Data::Bool(true)), "True");
        assert_eq!(cell_to_string(&Data::Bool(false)), "False");
    }

    #[test]
    fn cell_to_string_passes_through_strings_and_numbers() {
        assert_eq!(cell_to_string(&Data::String("hi".to_string())), "hi");
        assert_eq!(cell_to_string(&Data::Int(42)), "42");
        assert_eq!(cell_to_string(&Data::Float(1.5)), "1.5");
    }

    #[test]
    fn cell_is_truthy_matches_python_truthiness() {
        assert!(!cell_is_truthy(&Data::Empty));
        assert!(!cell_is_truthy(&Data::Int(0)));
        assert!(cell_is_truthy(&Data::Int(1)));
        assert!(!cell_is_truthy(&Data::Float(0.0)));
        assert!(cell_is_truthy(&Data::Float(0.1)));
        assert!(!cell_is_truthy(&Data::Bool(false)));
        assert!(cell_is_truthy(&Data::Bool(true)));
        assert!(!cell_is_truthy(&Data::String(String::new())));
        assert!(cell_is_truthy(&Data::String("x".to_string())));
    }
}
