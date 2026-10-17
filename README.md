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

## Development

Each tool is a crate under `src/`; any crate there with a `src/main.rs` is
picked up by `install.sh`. Shared error handling lives in `src/common` and CLI
test helpers in `src/testutil`. Run the tests with:

```
cargo test --workspace
```

## `guid`
Generates GUIDs (v4), lowercase unless `-u` is given.
```
Generates GUIDs (v4)

Usage: guid [OPTIONS]

Options:
  -c, --count <COUNT>                Number of GUIDs to generate [default: 1]
  -i, --from-integer <FROM_INTEGER>  Generate a GUID from a 128-bit integer
  -e, --empty                        Generate an empty GUID (all bits 0)
  -u, --uppercase                    Use uppercase letters
  -l, --lowercase                    Use lowercase letters (default)
  -h, --help                         Print help
```

## `urandom`
Generates a random string of `--size` characters drawn uniformly from
`--charset`, or random bytes printed as hex (`-x`) or written raw (`-b`).
```
Generate random data

Usage: urandom [OPTIONS]

Options:
  -i, --size <SIZE>        Number of characters to generate (bytes with --hex or --bytes) [default: 32]
  -c, --charset <CHARSET>  Character set to use [default: abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789]
  -x, --hex                Output as HEX
  -b, --bytes              Output as raw bytes
  -h, --help               Print help
```

## `mapit`
Maps each line from stdin to a value from a mapping file of `key:value`
lines. Keys, values and input lines are trimmed; lines with no mapping print
the `--default-value`.
```
Maps lines from stdin using a mapping file

Usage: mapit [OPTIONS] --input-mapping <INPUT_MAPPING>

Options:
  -i, --input-mapping <INPUT_MAPPING>  File containing the mapping
  -d, --map-delimiter <MAP_DELIMITER>  Delimiter used in the mapping file [default: :]
  -e, --default-value <DEFAULT_VALUE>  Default value to use if no mapping is found [default: ""]
  -h, --help                           Print help
```

## `textsplit`
Splits each line from stdin on a delimiter and prints each part on its own
line. Parts are trimmed unless `-T` is given.
```
Splits text into lines based on a delimiter

Usage: textsplit [OPTIONS]

Options:
  -d, --delimiter <DELIMITER>  Delimiter to split on [default: ,]
  -n, --ignore-empty           Do not write empty lines
  -T, --no-trim                Do not trim whitespace from lines
  -h, --help                   Print help
```

## `zargs`
A simpler `xargs`: runs the command once per line from stdin. Each line is
split on spaces, and `{0}`, `{1}`, etc. in the command are replaced with the
parts (`{{` and `}}` give literal braces), e.g.
`echo "a b c" | zargs -- echo "{1} {0} {2}"` prints `b a c`.
```
A simpler xargs. Use positional arguments in the command with {0}, {1} e.g. `echo "a b c" | zargs -- echo "{1} {0} {2}"`

Usage: zargs <COMMAND>...

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
than the current file, nothing is done. The output is only written once it's
complete, so a failed run never leaves a broken file - even with `--in-place`.
```
Crushes an EPUB's images down until the file hits a target size

Usage: epubc [OPTIONS] --target-size <TARGET_SIZE> <INPUT>

Arguments:
  <INPUT>  EPUB file to crush

Options:
  -t, --target-size <TARGET_SIZE>  Target size, as a percentage of the current size (e.g. "30%") or an absolute size (e.g. "10MB", "500KB", "1.5GB"). If this is an absolute size that's not smaller than the current file, nothing is done
  -o, --output <OUTPUT>            Output file (default: <input-stem>.crushed.epub next to the input)
      --in-place                   Overwrite the input file instead of writing a new one
  -h, --help                       Print help (see more with '--help')
```
Example: `epubc book.epub --target-size 30%` or `epubc book.epub --target-size 10MB`.

## `pathsize`
Prints the size of a file, or the total size of a directory's contents
(recursively, without following symlinks). Defaults to bytes; unit flags
follow standard Unix conventions (`-k`/`-m`/`-g`/`-t` for a fixed unit, `-h`
to auto-scale like `du -h`/`ls -lh`), printed as e.g. `4.0K`, `2.9M`, `1.2G`.
```
Prints the size of a file, or the total size of a directory's contents

Usage: pathsize [OPTIONS] [PATH]

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
Prints columns of an Excel sheet, one row per line. Columns are given as
1-based indexes or letters (`-c 1 3` or `-c A C`); only those with a value in
the first printed row are included, and empty cells in later rows print the
`--default-value`. The header row is skipped unless `-W` is given. Note that
`-c` defaults to `0`, which matches no column.
```
Prints the columns of an Excel sheet

Usage: colsof [OPTIONS] --sheet-name <SHEET_NAME> <FILE>

Arguments:
  <FILE>  Excel file to read

Options:
  -i, --sheet-name <SHEET_NAME>        Name or index of the sheet to read
  -w, --skip-header                    Skip the first row of the sheet (default)
  -W, --no-skip-header                 Print the first row of the sheet too
  -d, --delimiter <DELIMITER>          Delimiter to use between columns [default: " "]
  -t, --trim                           Trim whitespace from columns
  -c, --columns <COLUMNS>...           Column to read (either as index or letter) [default: 0]
  -f, --default-value <DEFAULT_VALUE>  Default value to use for empty cells [default: <EMPTY>]
  -h, --help                           Print help
```

## `dtail`
Tails logs from all running Docker containers matching any pattern - like
[`kubetail`](https://github.com/johanhaleby/kubetail), but for local Docker.
Each line is prefixed with a coloured, aligned label. Compose containers are
labelled (and matched) by service name, e.g. `kyberlifeorder` rather than
`myproject-kyberlifeorder-1`. It keeps watching for containers that start,
restart or are recreated later: new ones are tailed from their first line, and
restarted ones carry on where they stopped. Uses the `docker` CLI, so the
active Docker context is respected.
```
Tails logs from all running Docker containers matching any pattern, like kubetail

Usage: dtail [OPTIONS] [PATTERNS]...

Arguments:
  [PATTERNS]...  Regexes to match containers against (any match is tailed)

Options:
  -s, --since <SINCE>        Show logs since a time or duration (e.g. 10m, 1h, 2026-10-08T09:00); overrides --tail
  -n, --tail <TAIL>          Lines of history to show per container at startup [default: 10]
  -t, --timestamps           Show Docker timestamps
  -x, --exclude <EXCLUDE>    Skip containers matching this regex (repeatable)
  -g, --grep <GREP>          Only print lines matching this regex (case-insensitive)
  -1, --no-watch             Don't watch for new, restarted, or recreated containers; exit when all logs end
  -f, --full-names           Use full container names (for prefix and matching), not compose service names
      --interval <INTERVAL>  Seconds between checks for new containers [default: 2]
  -h, --help                 Print help
```
Example: `dtail order iam`, `dtail -x elastic -s 5m`, or `dtail -g 'error|exception'`.
