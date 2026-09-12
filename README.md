# CLI Utilities
Useful CLI utilities, written in Rust.

## Install

Requires [Rust](https://rustup.rs). Then run:

```
./install.sh
```

This builds each tool in release mode and installs the binaries to
`/usr/local/bin` (already on `PATH` by default on macOS and Debian/Ubuntu),
using `sudo` if needed to write there. The script offers to install `rustup`
first if `cargo` isn't found.

## `guid`
Generates GUIDs
```
usage: guid [OPTIONS]

Generates GUIDs (v4)

Options:
  -c, --count <COUNT>                Number of GUIDs to generate [default: 1]
  -i, --from-integer <FROM_INTEGER>  Generate a GUID from a 128-bit integer
  -e, --empty                        Generate an empty GUID (all bits 0)
  -u, --uppercase                    Use uppercase letters
  -l, --lowercase                    Use lowercase letters
  -h, --help                         Print help
```

## `urandom`
Generates a random string
```
usage: urandom [OPTIONS]

Generate random data

Options:
  -i, --size <SIZE>        Number of bytes to generate [default: 32]
  -c, --charset <CHARSET>  Character set to use [default: abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789]
  -x, --hex                Output as HEX
  -b, --bytes              Output as bytes
  -h, --help                Print help
```

## `mapit`
Maps lines from stdin using a mapping file
```
usage: mapit [OPTIONS] --input-mapping <INPUT_MAPPING>

Maps lines from stdin using a mapping file

Options:
  -i, --input-mapping <INPUT_MAPPING>  File containing the mapping
  -d, --map-delimiter <MAP_DELIMITER>  Delimiter used in the mapping file [default: :]
  -e, --default-value <DEFAULT_VALUE>  Default value to use if no mapping is found [default: ""]
  -h, --help                           Print help
```

## `textsplit`
Splits text into lines based on a delimiter
```
usage: textsplit [OPTIONS]

Splits text into lines based on a delimiter

Options:
  -d, --delimiter <DELIMITER>  Delimiter to split on [default: ,]
  -n, --ignore-empty           Ignore empty lines
  -w, --no-empty-write         Do not write empty lines
  -t, --trim                   Trim whitespace from lines
  -h, --help                   Print help
```

## `zargs`
A simpler `xargs`. Use positional arguments in the command with `{0}`, `{1}`,
etc, e.g. `echo "a b c" | zargs -- echo "{1} {0} {2}"`.
```
usage: zargs <COMMAND>...

Arguments:
  <COMMAND>...  Command to run

Options:
  -h, --help  Print help
```

## `epubc`
Crushes an EPUB's images down until the file hits a target size - useful
for making image-heavy EPUBs (scanned books, comics) less laggy on
e-readers. Text, fonts, and markup are left untouched; only JPEG quality
and, if that alone isn't enough, image resolution (for both JPEG and PNG)
are reduced. Image formats are never converted, so the OPF manifest's
declared media types stay valid. If an absolute target size isn't smaller
than the current file, nothing is done.
```
usage: epubc [OPTIONS] --target-size <TARGET_SIZE> <INPUT>

Crushes an EPUB's images down until the file hits a target size

Arguments:
  <INPUT>  EPUB file to crush

Options:
  -t, --target-size <TARGET_SIZE>  Target size, as a percentage of the current size (e.g. "30%") or an absolute size (e.g. "10MB", "500KB", "1.5GB")
  -o, --output <OUTPUT>            Output file (default: <input-stem>.crushed.epub next to the input)
      --in-place                   Overwrite the input file instead of writing a new one
  -h, --help                       Print help
```
Example: `epubc book.epub --target-size 30%` or `epubc book.epub --target-size 10MB`.

## `pathsize`
Prints the size of a file, or the total size of a directory's contents
(recursively). Defaults to bytes; unit flags follow standard Unix
conventions (`-k`/`-m`/`-g`/`-t` for a fixed unit, `-h` to auto-scale like
`du -h`/`ls -lh`), printed as e.g. `4.0K`, `2.9M`, `1.2G`.
```
usage: pathsize [OPTIONS] [PATH]

Prints the size of a file, or the total size of a directory's contents

Arguments:
  [PATH]  Path to measure [default: .]

Options:
  -b, --bytes           Print size in bytes (default)
  -k, --kilo            Print size in kibibytes (1024 bytes)
  -m, --mega            Print size in mebibytes (1024^2 bytes)
  -g, --giga            Print size in gibibytes (1024^3 bytes)
  -t, --tera            Print size in tebibytes (1024^4 bytes)
  -h, --human-readable  Automatically scale to the largest unit that keeps the value readable
      --help            Print help
```

## `colsof`
Prints the columns of an Excel sheet
```
usage: colsof [OPTIONS] --sheet-name <SHEET_NAME> <FILE>

Prints the columns of an Excel sheet

Arguments:
  <FILE>  Excel file to read

Options:
  -i, --sheet-name <SHEET_NAME>        Name or index of the sheet to read
  -w, --skip-header                    Skip the first row of the sheet
  -d, --delimiter <DELIMITER>          Delimiter to use between columns [default: " "]
  -t, --trim                           Trim whitespace from columns
  -c, --columns <COLUMNS>...           Column to read (either as index or letter) [default: 0]
  -f, --default-value <DEFAULT_VALUE>  Default value to use for empty cells [default: <EMPTY>]
  -h, --help                           Print help
```
