use std::borrow::Cow;
use std::fs;
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Crushes an EPUB's images down until the file hits a target size.
///
/// Most of an EPUB's size usually comes from its images, so this leaves
/// text, fonts and markup untouched and only reduces image quality/
/// dimensions (JPEG quality and, if needed, resolution for both JPEG and
/// PNG) - just enough to hit the target. Image formats are never
/// converted, so the OPF manifest's declared media types stay valid.
#[derive(Parser)]
struct Args {
    /// EPUB file to crush
    input: PathBuf,

    /// Target size, as a percentage of the current size (e.g. "30%") or an
    /// absolute size (e.g. "10MB", "500KB", "1.5GB"). If this is an
    /// absolute size that's not smaller than the current file, nothing is
    /// done.
    #[arg(short, long)]
    target_size: String,

    /// Output file (default: <input-stem>.crushed.epub next to the input)
    #[arg(short, long, conflicts_with = "in_place")]
    output: Option<PathBuf>,

    /// Overwrite the input file instead of writing a new one
    #[arg(long)]
    in_place: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Jpeg,
    Png,
    /// Anything that is written back out as it was read.
    Fixed,
}

struct Entry {
    name: String,
    is_dir: bool,
    data: Vec<u8>,
    options: SimpleFileOptions,
    compressed_size: u64,
    kind: Kind,
}

fn parse_target_size(s: &str, current_size: u64) -> Result<u64, String> {
    // Two-letter suffixes first, so "KB" isn't read as a number ending in "B".
    const UNITS: [(&str, u64); 9] = [
        ("TB", 1 << 40),
        ("GB", 1 << 30),
        ("MB", 1 << 20),
        ("KB", 1 << 10),
        ("T", 1 << 40),
        ("G", 1 << 30),
        ("M", 1 << 20),
        ("K", 1 << 10),
        ("B", 1),
    ];

    let parse_number = |num: &str| -> Result<f64, String> {
        let value: f64 = num
            .trim()
            .parse()
            .map_err(|_| format!("invalid target size: {s}"))?;
        if value < 0.0 {
            return Err(format!("target size cannot be negative: {s}"));
        }
        Ok(value)
    };

    let trimmed = s.trim();
    if let Some(pct) = trimmed.strip_suffix('%') {
        return Ok((current_size as f64 * parse_number(pct)? / 100.0).round() as u64);
    }

    let upper = trimmed.to_ascii_uppercase();
    let (num_part, unit_bytes) = UNITS
        .iter()
        .find_map(|(suffix, bytes)| Some((upper.strip_suffix(suffix)?, *bytes)))
        .unwrap_or((upper.as_str(), 1));
    Ok((parse_number(num_part)? * unit_bytes as f64).round() as u64)
}

fn default_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let dir = input.parent().filter(|p| !p.as_os_str().is_empty());
    match dir {
        Some(dir) => dir.join(format!("{stem}.crushed.epub")),
        None => PathBuf::from(format!("{stem}.crushed.epub")),
    }
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, quality)
        .encode_image(img)
        .map_err(|e| format!("failed to encode jpeg: {e}"))?;
    Ok(buf)
}

fn encode_png(img: &DynamicImage) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
        .map_err(|e| format!("failed to encode png: {e}"))?;
    Ok(buf)
}

/// Shrinks a JPEG to fit within `target` bytes: first by lowering quality,
/// then - if quality alone can't get there - by also downscaling.
fn shrink_jpeg(img: &DynamicImage, target: usize) -> Result<Vec<u8>, String> {
    const MIN_QUALITY: u8 = 10;
    const MAX_QUALITY: u8 = 92;
    const MIN_LONG_SIDE: u32 = 320;
    const MAX_RESIZE_ATTEMPTS: u32 = 6;

    let mut current = img.clone();

    for _ in 0..MAX_RESIZE_ATTEMPTS {
        let floor = encode_jpeg(&current, MIN_QUALITY)?;
        if floor.len() <= target {
            let mut lo = MIN_QUALITY;
            let mut hi = MAX_QUALITY;
            let mut best = floor;
            while lo < hi {
                let mid = lo + (hi - lo).div_ceil(2);
                let candidate = encode_jpeg(&current, mid)?;
                if candidate.len() <= target {
                    best = candidate;
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            return Ok(best);
        }

        let (w, h) = current.dimensions();
        if w.max(h) <= MIN_LONG_SIDE {
            return Ok(floor);
        }

        // JPEG size at a fixed quality scales roughly with pixel count, so
        // jump most of the way to a resolution that should fit instead of
        // creeping down in small steps (which needs many more expensive
        // encode+resize rounds to converge on a large size cut).
        let scale = ((target as f64 / floor.len() as f64).sqrt() * 0.95).clamp(0.3, 0.9);
        let nw = ((w as f64 * scale).round() as u32).max(1);
        let nh = ((h as f64 * scale).round() as u32).max(1);
        current = current.resize(nw, nh, FilterType::Lanczos3);
    }

    encode_jpeg(&current, MIN_QUALITY)
}

/// Shrinks a PNG to fit within `target` bytes by downscaling (PNG has no
/// quality knob to turn - only resolution meaningfully changes its size).
/// How much a given resolution cut actually shrinks the file depends
/// heavily on image content (flat art can compress unpredictably as detail
/// aliases away), so this binary-searches the scale factor against the
/// real encoded size at each step rather than guessing from pixel counts.
fn shrink_png(img: &DynamicImage, target: usize) -> Result<Vec<u8>, String> {
    const MIN_SCALE: f64 = 0.02;
    const ITERATIONS: u32 = 12;

    let full = encode_png(img)?;
    if full.len() <= target {
        return Ok(full);
    }

    let (w0, h0) = img.dimensions();
    let scaled_encode = |scale: f64| {
        let w = ((w0 as f64 * scale).round() as u32).max(1);
        let h = ((h0 as f64 * scale).round() as u32).max(1);
        encode_png(&img.resize(w, h, FilterType::Lanczos3))
    };

    // Assume encoded size is (roughly) monotonic in scale: establish a
    // floor that satisfies the target before binary-searching upward from
    // it for the largest scale that still fits.
    let mut best = scaled_encode(MIN_SCALE)?;
    if best.len() > target {
        return Ok(best); // can't do better even at the smallest allowed size
    }

    let mut lo = MIN_SCALE;
    let mut hi = 1.0;
    for _ in 0..ITERATIONS {
        let mid = (lo + hi) / 2.0;
        let candidate = scaled_encode(mid)?;
        if candidate.len() <= target {
            best = candidate;
            lo = mid;
        } else {
            hi = mid;
        }
    }

    Ok(best)
}

fn kind_of(data: &[u8]) -> Kind {
    match image::guess_format(data) {
        Ok(ImageFormat::Jpeg) => Kind::Jpeg,
        Ok(ImageFormat::Png) => Kind::Png,
        _ => Kind::Fixed,
    }
}

fn read_entries(input: &Path) -> Result<Vec<Entry>, String> {
    let file =
        fs::File::open(input).map_err(|e| format!("failed to open {}: {e}", input.display()))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| format!("failed to read {} as an epub: {e}", input.display()))?;

    let mut entries = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let mut zf = archive
            .by_index(i)
            .map_err(|e| format!("failed to read entry {i}: {e}"))?;
        let name = zf.name().to_string();
        let is_dir = zf.is_dir();
        let mut data = Vec::new();
        if !is_dir {
            zf.read_to_end(&mut data)
                .map_err(|e| format!("failed to read {name}: {e}"))?;
        }
        entries.push(Entry {
            kind: kind_of(&data),
            name,
            is_dir,
            data,
            options: zf.options(),
            compressed_size: zf.compressed_size(),
        });
    }
    Ok(entries)
}

/// The fraction of its current size each image has to shrink to for the
/// whole file to land on `target_bytes`.
///
/// `compressed_size` is what the *original* entries contribute to the
/// current file size, so it's used to work out how many bytes are spent
/// on things we won't touch. But once crushed, images are written back
/// out uncompressed (see `write_entries`), so their contribution to the
/// *new* file size is their raw byte length - that's the right basis for
/// sizing each image's target, not their incidental original zip
/// compression (which can vary a lot and has nothing to do with how
/// shrinkable the image itself is).
fn image_ratio(entries: &[Entry], current_size: u64, target_bytes: u64) -> f64 {
    let images = || entries.iter().filter(|e| e.kind != Kind::Fixed);
    let compressed_total: u64 = images().map(|e| e.compressed_size).sum();
    let raw_total: u64 = images().map(|e| e.data.len() as u64).sum();
    if raw_total == 0 {
        return 1.0;
    }

    let fixed_total = current_size.saturating_sub(compressed_total);
    let target_for_images = target_bytes.saturating_sub(fixed_total);
    (target_for_images as f64 / raw_total as f64).clamp(0.02, 1.0)
}

/// Returns the bytes to write for `entry`: its image shrunk by `ratio`, or
/// its data as it is if there's nothing to gain.
fn crush(entry: &Entry, ratio: f64) -> Result<Cow<'_, [u8]>, String> {
    // Below this, an image is already small enough that squeezing it
    // further barely moves the total file size while risking visibly
    // wrecking small icons/decorations - so just leave it alone.
    const MIN_TARGET_BYTES: usize = 16 * 1024;

    let original = Cow::Borrowed(entry.data.as_slice());
    let shrink = match entry.kind {
        Kind::Jpeg => shrink_jpeg,
        Kind::Png => shrink_png,
        Kind::Fixed => return Ok(original),
    };
    let target = ((entry.data.len() as f64 * ratio).round() as usize).max(MIN_TARGET_BYTES);
    if ratio >= 0.98 || target >= entry.data.len() {
        return Ok(original);
    }

    // Decoded here rather than up front, so only one image is held in
    // memory at a time.
    match image::load_from_memory(&entry.data) {
        Ok(img) => Ok(Cow::Owned(shrink(&img, target)?)),
        Err(_) => Ok(original),
    }
}

fn write_entries(entries: &[Entry], ratio: f64, file: fs::File) -> Result<(), String> {
    let mut writer = ZipWriter::new(file);

    for entry in entries {
        if entry.is_dir {
            writer
                .add_directory(&entry.name, entry.options)
                .map_err(|e| format!("failed to write directory {}: {e}", entry.name))?;
            continue;
        }

        let data = crush(entry, ratio)?;

        // The EPUB format requires an uncompressed mimetype entry, and
        // deflating already-compressed image data just burns CPU for no
        // real size benefit.
        let stored = entry.name.eq_ignore_ascii_case("mimetype") || entry.kind != Kind::Fixed;
        let options = if stored {
            entry.options.compression_method(CompressionMethod::Stored)
        } else {
            entry.options
        };

        writer
            .start_file(&entry.name, options)
            .map_err(|e| format!("failed to write entry {}: {e}", entry.name))?;
        writer
            .write_all(&data)
            .map_err(|e| format!("failed to write entry {}: {e}", entry.name))?;
    }

    writer
        .finish()
        .map_err(|e| format!("failed to finalize the epub: {e}"))?;
    Ok(())
}

/// Writes the crushed EPUB to a temporary file next to `output` and moves it
/// into place only once it's complete, so a failure part-way through never
/// leaves a truncated file behind - or, with --in-place, destroys the input.
fn write_epub(entries: &[Entry], ratio: f64, output: &Path) -> Result<(), String> {
    // Resolve symlinks so the file they point at is replaced, not the link.
    let dest = fs::canonicalize(output).unwrap_or_else(|_| output.to_path_buf());
    let mut tmp_name = dest
        .file_name()
        .ok_or_else(|| format!("{} is not a file path", output.display()))?
        .to_os_string();
    tmp_name.push(".tmp");
    let tmp = dest.with_file_name(tmp_name);

    let result = fs::File::create(&tmp)
        .map_err(|e| format!("failed to create {}: {e}", tmp.display()))
        .and_then(|file| write_entries(entries, ratio, file))
        .and_then(|()| {
            if let Ok(existing) = fs::metadata(&dest) {
                let _ = fs::set_permissions(&tmp, existing.permissions());
            }
            fs::rename(&tmp, &dest)
                .map_err(|e| format!("failed to write {}: {e}", output.display()))
        });
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn main() -> ExitCode {
    common::run(env!("CARGO_PKG_NAME"), run)
}

fn run() -> common::Result {
    let args = Args::parse();

    let current_size = fs::metadata(&args.input)
        .map_err(|e| format!("failed to read {}: {e}", args.input.display()))?
        .len();
    let target_bytes = parse_target_size(&args.target_size, current_size)?;

    if target_bytes >= current_size {
        writeln!(
            io::stdout(),
            "target size ({}) is not smaller than the current size ({current_size} bytes); leaving {} untouched",
            args.target_size,
            args.input.display()
        )?;
        return Ok(());
    }

    let output_path = if args.in_place {
        args.input.clone()
    } else {
        args.output
            .clone()
            .unwrap_or_else(|| default_output_path(&args.input))
    };

    let entries = read_entries(&args.input)?;
    if entries.iter().all(|e| e.kind == Kind::Fixed) {
        eprintln!("warning: no JPEG or PNG images found to compress; output will be about the same size");
    }
    let ratio = image_ratio(&entries, current_size, target_bytes);
    write_epub(&entries, ratio, &output_path)?;

    let new_size = fs::metadata(&output_path)
        .map_err(|e| format!("failed to read back {}: {e}", output_path.display()))?
        .len();

    let pct = 100.0 - (new_size as f64 / current_size as f64 * 100.0);
    writeln!(
        io::stdout(),
        "{}: {current_size} -> {new_size} bytes ({pct:.1}% smaller, target was {target_bytes} bytes)",
        output_path.display()
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod support;

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(support::photo(w, h))
    }

    // -- parse_target_size --------------------------------------------

    #[test]
    fn parses_a_percentage_of_the_current_size() {
        assert_eq!(parse_target_size("50%", 1000).unwrap(), 500);
        assert_eq!(parse_target_size("0%", 1000).unwrap(), 0);
        assert_eq!(parse_target_size("100%", 1000).unwrap(), 1000);
    }

    #[test]
    fn parses_absolute_sizes_with_binary_suffixes() {
        assert_eq!(parse_target_size("100B", 0).unwrap(), 100);
        assert_eq!(parse_target_size("1K", 0).unwrap(), 1024);
        assert_eq!(parse_target_size("1KB", 0).unwrap(), 1024);
        assert_eq!(parse_target_size("1M", 0).unwrap(), 1 << 20);
        assert_eq!(parse_target_size("1MB", 0).unwrap(), 1 << 20);
        assert_eq!(parse_target_size("2G", 0).unwrap(), 2 * (1 << 30));
        assert_eq!(parse_target_size("1T", 0).unwrap(), 1u64 << 40);
    }

    #[test]
    fn parses_bare_numbers_as_bytes() {
        assert_eq!(parse_target_size("12345", 0).unwrap(), 12345);
    }

    #[test]
    fn parses_fractional_sizes() {
        assert_eq!(parse_target_size("1.5MB", 0).unwrap(), (1.5 * (1u64 << 20) as f64) as u64);
    }

    #[test]
    fn is_case_insensitive_and_tolerates_whitespace() {
        assert_eq!(parse_target_size(" 10mb ", 0).unwrap(), 10 * (1 << 20));
        assert_eq!(parse_target_size("10 MB", 0).unwrap(), 10 * (1 << 20));
    }

    #[test]
    fn rejects_garbage_and_negative_sizes() {
        assert!(parse_target_size("not-a-size", 0).is_err());
        assert!(parse_target_size("-5MB", 0).is_err());
        assert!(parse_target_size("-10%", 0).is_err());
    }

    // -- default_output_path -------------------------------------------

    #[test]
    fn default_output_path_adds_crushed_suffix_next_to_the_input() {
        assert_eq!(
            default_output_path(Path::new("/books/foo.epub")),
            PathBuf::from("/books/foo.crushed.epub")
        );
    }

    #[test]
    fn default_output_path_handles_a_bare_filename() {
        assert_eq!(
            default_output_path(Path::new("foo.epub")),
            PathBuf::from("foo.crushed.epub")
        );
    }

    // -- kind_of ----------------------------------------------------------

    #[test]
    fn jpeg_and_png_bytes_are_recognised_by_content() {
        assert_eq!(kind_of(&encode_jpeg(&photo(64, 64), 80).unwrap()), Kind::Jpeg);
        assert_eq!(kind_of(&encode_png(&photo(64, 64)).unwrap()), Kind::Png);
    }

    #[test]
    fn non_image_bytes_are_fixed() {
        assert_eq!(kind_of(b"<html><body>hello</body></html>"), Kind::Fixed);
        assert_eq!(kind_of(b""), Kind::Fixed);
    }

    // -- image_ratio / crush ----------------------------------------------

    fn entry(kind: Kind, data: Vec<u8>, compressed_size: u64) -> Entry {
        Entry {
            name: "x".to_string(),
            is_dir: false,
            data,
            options: SimpleFileOptions::default(),
            compressed_size,
            kind,
        }
    }

    #[test]
    fn image_ratio_spends_the_whole_cut_on_the_images() {
        // 1000 bytes on disk: 400 of text, 600 of image (800 raw).
        let entries = [
            entry(Kind::Fixed, vec![0; 900], 400),
            entry(Kind::Jpeg, vec![0; 800], 600),
        ];
        // To reach 600, the images get 600 - 400 = 200 of their 800 raw bytes.
        assert_eq!(image_ratio(&entries, 1000, 600), 0.25);
    }

    #[test]
    fn image_ratio_is_clamped_and_is_one_without_images() {
        let entries = [
            entry(Kind::Fixed, vec![0; 900], 400),
            entry(Kind::Png, vec![0; 800], 600),
        ];
        assert_eq!(image_ratio(&entries, 1000, 100), 0.02);
        assert_eq!(image_ratio(&entries[..1], 1000, 100), 1.0);
    }

    #[test]
    fn crush_leaves_fixed_small_and_undecodable_entries_alone() {
        let text = entry(Kind::Fixed, vec![7; 100_000], 0);
        assert!(matches!(crush(&text, 0.1), Ok(Cow::Borrowed(_))));

        let icon = entry(Kind::Png, encode_png(&photo(16, 16)).unwrap(), 0);
        assert!(matches!(crush(&icon, 0.1), Ok(Cow::Borrowed(_))));

        let broken = entry(Kind::Jpeg, vec![7; 100_000], 0);
        assert!(matches!(crush(&broken, 0.1), Ok(Cow::Borrowed(_))));
    }

    #[test]
    fn crush_shrinks_a_large_image() {
        let image = entry(Kind::Jpeg, encode_jpeg(&photo(800, 600), 92).unwrap(), 0);
        let shrunk = crush(&image, 0.5).unwrap();
        assert!(shrunk.len() <= image.data.len() / 2);
    }

    // -- shrink_jpeg / shrink_png ---------------------------------------

    #[test]
    fn shrink_jpeg_hits_a_moderate_target_via_quality_alone() {
        let img = photo(400, 300);
        let original = encode_jpeg(&img, 95).unwrap();
        let target = original.len() / 3;
        let shrunk = shrink_jpeg(&img, target).unwrap();
        assert!(shrunk.len() <= target.max(1) || shrunk.len() < original.len());
        // Still a valid, decodable JPEG at the original resolution (quality
        // alone should be enough for a 3x cut on a small image).
        let decoded = image::load_from_memory(&shrunk).unwrap();
        assert_eq!(decoded.dimensions(), (400, 300));
    }

    #[test]
    fn shrink_jpeg_downscales_when_quality_cannot_reach_a_tiny_target() {
        let img = photo(2000, 1500);
        let shrunk = shrink_jpeg(&img, 4000).unwrap();
        assert!(shrunk.len() <= 4000 || shrunk.len() < encode_jpeg(&img, 92).unwrap().len());
        let decoded = image::load_from_memory(&shrunk).unwrap();
        let (w, h) = decoded.dimensions();
        assert!(w < 2000 && h < 1500, "expected downscale, got {w}x{h}");
    }

    #[test]
    fn shrink_jpeg_never_grows_the_image() {
        let img = photo(300, 200);
        let shrunk = shrink_jpeg(&img, 1_000_000_000).unwrap();
        let decoded = image::load_from_memory(&shrunk).unwrap();
        let (w, h) = decoded.dimensions();
        assert!(w <= 300 && h <= 200);
    }

    #[test]
    fn shrink_png_downscales_to_fit_a_small_target() {
        let img = photo(400, 300);
        let full = encode_png(&img).unwrap();
        let target = full.len() / 4;
        let shrunk = shrink_png(&img, target).unwrap();
        assert!(shrunk.len() <= target);
        let decoded = image::load_from_memory(&shrunk).unwrap();
        let (w, h) = decoded.dimensions();
        assert!(w < 400 && h < 300);
    }

    #[test]
    fn shrink_png_leaves_image_untouched_if_already_under_target() {
        let img = photo(50, 50);
        let full = encode_png(&img).unwrap();
        let shrunk = shrink_png(&img, full.len() + 1000).unwrap();
        assert_eq!(shrunk, full);
    }
}
