use std::collections::BTreeSet;

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

    /// Skip the first row of the sheet
    #[arg(short = 'w', long, default_value_t = true)]
    skip_header: bool,

    /// Delimiter to use between columns
    #[arg(short, long, default_value = " ")]
    delimiter: String,

    /// Trim whitespace from columns
    #[arg(short, long)]
    trim: bool,

    /// Column to read (either as index or letter)
    #[arg(short, long, num_args = 1.., default_values_t = ["0".to_string()])]
    columns: Vec<String>,

    /// Default value to use for empty cells
    #[arg(short = 'f', long, default_value = "<EMPTY>")]
    default_value: String,
}

fn col_to_index(col: &str) -> u32 {
    if col.chars().all(|c| c.is_ascii_digit()) && !col.is_empty() {
        return col.parse().expect("column index out of range");
    }
    let mut index: u32 = 0;
    for c in col.to_ascii_uppercase().chars() {
        index = index * 26 + (c as u32 - 'A' as u32) + 1;
    }
    index
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

fn main() {
    let args = Args::parse();

    let mut converted_columns: Vec<u32> = args.columns.iter().map(|c| col_to_index(c)).collect();
    converted_columns.sort_unstable();
    let allowed_columns: BTreeSet<u32> = converted_columns.into_iter().collect();

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
    if args.skip_header {
        rows.next();
    }

    let mut final_columns: BTreeSet<u32> = BTreeSet::new();
    let mut first = true;
    let mut values: Vec<String> = Vec::new();

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
        println!("{}", values.join(&args.delimiter));
        first = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn col_to_index_parses_plain_numbers_as_is() {
        assert_eq!(col_to_index("0"), 0);
        assert_eq!(col_to_index("5"), 5);
    }

    #[test]
    fn col_to_index_converts_single_letters() {
        assert_eq!(col_to_index("A"), 1);
        assert_eq!(col_to_index("a"), 1);
        assert_eq!(col_to_index("Z"), 26);
    }

    #[test]
    fn col_to_index_converts_multi_letter_columns() {
        assert_eq!(col_to_index("AA"), 27);
        assert_eq!(col_to_index("AZ"), 52);
        assert_eq!(col_to_index("BA"), 53);
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
