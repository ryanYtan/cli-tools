use std::collections::BTreeSet;
use std::io::{self, Write};
use std::process::ExitCode;

use calamine::{open_workbook_auto, Data, DataType, Reader};
use clap::Parser;

/// Prints the columns of an Excel sheet
#[derive(Parser)]
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

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn col_to_index(col: &str) -> Result<u32, String> {
    let out_of_range = || format!("column {col} is out of range");
    if is_number(col) {
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

/// The 1-based column number of each cell in a row of a range that starts at
/// column index `start_col`.
fn numbered(row: &[Data], start_col: u32) -> impl Iterator<Item = (u32, &Data)> {
    (start_col + 1..).zip(row)
}

/// The requested columns that have a value in the first row printed. Only
/// these are printed, for every row.
fn columns_present(first_row: &[Data], start_col: u32, requested: &BTreeSet<u32>) -> BTreeSet<u32> {
    numbered(first_row, start_col)
        .filter(|(col, cell)| requested.contains(col) && !cell.is_empty())
        .map(|(col, _)| col)
        .collect()
}

/// A row's values for the given columns, with `default_value` standing in
/// for cells that are empty or otherwise falsy.
fn row_values(
    row: &[Data],
    start_col: u32,
    columns: &BTreeSet<u32>,
    trim: bool,
    default_value: &str,
) -> Vec<String> {
    numbered(row, start_col)
        .filter(|(col, _)| columns.contains(col))
        .map(|(_, cell)| {
            if !cell_is_truthy(cell) {
                return default_value.to_string();
            }
            let s = cell_to_string(cell);
            if trim { s.trim().to_string() } else { s }
        })
        .collect()
}

fn main() -> ExitCode {
    common::run(env!("CARGO_PKG_NAME"), run)
}

fn run() -> common::Result {
    let args = Args::parse();

    let requested_columns: BTreeSet<u32> = args.columns.iter().copied().collect();

    let mut workbook = open_workbook_auto(&args.file)
        .map_err(|e| format!("failed to open {}: {e}", args.file))?;

    let range = if is_number(&args.sheet_name) {
        let no_sheet = || format!("no sheet at index {}", args.sheet_name);
        let sheet_index: usize = args.sheet_name.parse().map_err(|_| no_sheet())?;
        workbook
            .worksheet_range_at(sheet_index)
            .ok_or_else(no_sheet)?
            .map_err(|e| format!("failed to read sheet {sheet_index}: {e}"))?
    } else {
        workbook
            .worksheet_range(&args.sheet_name)
            .map_err(|e| format!("failed to read sheet {}: {e}", args.sheet_name))?
    };

    let Some((_, start_col)) = range.start() else {
        return Ok(());
    };

    let mut rows = range.rows().peekable();
    if !args.no_skip_header {
        rows.next();
    }
    let Some(first_row) = rows.peek() else {
        return Ok(());
    };
    let columns = columns_present(first_row, start_col, &requested_columns);

    let mut out = io::stdout().lock();
    for row in rows {
        let values = row_values(row, start_col, &columns, args.trim, &args.default_value);
        writeln!(out, "{}", values.join(&args.delimiter))?;
    }
    Ok(())
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

    fn text(s: &str) -> Data {
        Data::String(s.to_string())
    }

    fn set(cols: &[u32]) -> BTreeSet<u32> {
        cols.iter().copied().collect()
    }

    #[test]
    fn columns_present_keeps_requested_columns_with_a_value_in_the_first_row() {
        let row = [text("a"), Data::Empty, text("c"), text("d")];
        assert_eq!(columns_present(&row, 0, &set(&[1, 2, 3, 9])), set(&[1, 3]));
    }

    #[test]
    fn columns_are_numbered_from_where_the_range_starts() {
        // A range starting at column C: its first cell is column 3.
        let row = [text("c"), text("d")];
        assert_eq!(columns_present(&row, 2, &set(&[1, 3, 4])), set(&[3, 4]));
        assert_eq!(row_values(&row, 2, &set(&[4]), false, "-"), ["d"]);
    }

    #[test]
    fn row_values_substitutes_the_default_for_falsy_cells() {
        let row = [text("a"), Data::Empty, Data::Int(0), Data::Int(7)];
        assert_eq!(
            row_values(&row, 0, &set(&[1, 2, 3, 4]), false, "N/A"),
            ["a", "N/A", "N/A", "7"]
        );
    }

    #[test]
    fn row_values_trims_only_when_asked() {
        let row = [text("  a "), text("b")];
        assert_eq!(row_values(&row, 0, &set(&[1]), false, ""), ["  a "]);
        assert_eq!(row_values(&row, 0, &set(&[1]), true, ""), ["a"]);
    }

    #[test]
    fn row_values_skips_cells_past_a_short_row() {
        assert!(row_values(&[text("a")], 0, &set(&[2]), false, "").is_empty());
    }
}
