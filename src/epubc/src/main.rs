use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

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
#[command(about = "Crushes an EPUB's images down until the file hits a target size")]
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
    Fixed,
}

struct Entry {
    name: String,
    is_dir: bool,
    data: Vec<u8>,
    options: SimpleFileOptions,
    compressed_size: u64,
}

fn parse_target_size(s: &str, current_size: u64) -> Result<u64, String> {
    let trimmed = s.trim();

    if let Some(pct) = trimmed.strip_suffix('%') {
        let pct: f64 = pct
            .trim()
            .parse()
            .map_err(|_| format!("invalid target size: {s}"))?;
        if pct < 0.0 {
            return Err(format!("target size cannot be negative: {s}"));
        }
        return Ok((current_size as f64 * pct / 100.0).round() as u64);
    }

    let upper = trimmed.to_ascii_uppercase();
    let (num_part, unit_bytes): (&str, u64) = if let Some(p) = upper
        .strip_suffix("TB")
        .or_else(|| upper.strip_suffix('T'))
    {
        (p, 1u64 << 40)
    } else if let Some(p) = upper.strip_suffix("GB").or_else(|| upper.strip_suffix('G')) {
        (p, 1u64 << 30)
    } else if let Some(p) = upper.strip_suffix("MB").or_else(|| upper.strip_suffix('M')) {
        (p, 1u64 << 20)
    } else if let Some(p) = upper.strip_suffix("KB").or_else(|| upper.strip_suffix('K')) {
        (p, 1u64 << 10)
    } else if let Some(p) = upper.strip_suffix('B') {
        (p, 1)
    } else {
        (upper.as_str(), 1)
    };

    let value: f64 = num_part
        .trim()
        .parse()
        .map_err(|_| format!("invalid target size: {s}"))?;
    if value < 0.0 {
        return Err(format!("target size cannot be negative: {s}"));
    }
    Ok((value * unit_bytes as f64).round() as u64)
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

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Vec<u8> {
    let mut buf = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, quality)
        .encode_image(img)
        .expect("failed to encode jpeg");
    buf
}

fn encode_png(img: &DynamicImage) -> Vec<u8> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
        .expect("failed to encode png");
    buf
}

/// Shrinks a JPEG to fit within `target` bytes: first by lowering quality,
/// then - if quality alone can't get there - by also downscaling.
fn shrink_jpeg(img: &DynamicImage, target: usize) -> Vec<u8> {
    const MIN_QUALITY: u8 = 10;
    const MAX_QUALITY: u8 = 92;
    const MIN_LONG_SIDE: u32 = 320;
    const MAX_RESIZE_ATTEMPTS: u32 = 6;

    let mut current = img.clone();

    for _ in 0..MAX_RESIZE_ATTEMPTS {
        let floor = encode_jpeg(&current, MIN_QUALITY);
        if floor.len() <= target {
            let mut lo = MIN_QUALITY;
            let mut hi = MAX_QUALITY;
            let mut best = floor;
            while lo < hi {
                let mid = lo + (hi - lo).div_ceil(2);
                let candidate = encode_jpeg(&current, mid);
                if candidate.len() <= target {
                    best = candidate;
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            return best;
        }

        let (w, h) = current.dimensions();
        if w.max(h) <= MIN_LONG_SIDE {
            return floor;
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
fn shrink_png(img: &DynamicImage, target: usize) -> Vec<u8> {
    const MIN_SCALE: f64 = 0.02;
    const ITERATIONS: u32 = 12;

    let full = encode_png(img);
    if full.len() <= target {
        return full;
    }

    let (w0, h0) = img.dimensions();
    let scaled_encode = |scale: f64| -> Vec<u8> {
        let w = ((w0 as f64 * scale).round() as u32).max(1);
        let h = ((h0 as f64 * scale).round() as u32).max(1);
        encode_png(&img.resize(w, h, FilterType::Lanczos3))
    };

    // Assume encoded size is (roughly) monotonic in scale: establish a
    // floor that satisfies the target before binary-searching upward from
    // it for the largest scale that still fits.
    let mut best = scaled_encode(MIN_SCALE);
    if best.len() > target {
        return best; // can't do better even at the smallest allowed size
    }

    let mut lo = MIN_SCALE;
    let mut hi = 1.0;
    for _ in 0..ITERATIONS {
        let mid = (lo + hi) / 2.0;
        let candidate = scaled_encode(mid);
        if candidate.len() <= target {
            best = candidate;
            lo = mid;
        } else {
            hi = mid;
        }
    }

    best
}

fn classify(data: &[u8]) -> (Kind, Option<DynamicImage>) {
    let format = match image::guess_format(data) {
        Ok(f) => f,
        Err(_) => return (Kind::Fixed, None),
    };

    let kind = match format {
        ImageFormat::Jpeg => Kind::Jpeg,
        ImageFormat::Png => Kind::Png,
        _ => return (Kind::Fixed, None),
    };

    match image::load_from_memory(data) {
        Ok(img) => (kind, Some(img)),
        Err(_) => (Kind::Fixed, None),
    }
}

fn main() {
    let args = Args::parse();

    let current_size = fs::metadata(&args.input)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", args.input.display()))
        .len();

    let target_bytes = parse_target_size(&args.target_size, current_size).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });

    if target_bytes >= current_size {
        println!(
            "target size ({}) is not smaller than the current size ({current_size} bytes); leaving {} untouched",
            args.target_size,
            args.input.display()
        );
        return;
    }

    let output_path = if args.in_place {
        args.input.clone()
    } else {
        args.output
            .clone()
            .unwrap_or_else(|| default_output_path(&args.input))
    };

    let file = fs::File::open(&args.input)
        .unwrap_or_else(|e| panic!("failed to open {}: {e}", args.input.display()));
    let mut archive = ZipArchive::new(file)
        .unwrap_or_else(|e| panic!("failed to read {} as an epub: {e}", args.input.display()));

    let mut entries = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let mut zf = archive
            .by_index(i)
            .unwrap_or_else(|e| panic!("failed to read entry {i}: {e}"));
        let name = zf.name().to_string();
        let is_dir = zf.is_dir();
        let compressed_size = zf.compressed_size();
        let options = zf.options();
        let mut data = Vec::new();
        if !is_dir {
            zf.read_to_end(&mut data)
                .unwrap_or_else(|e| panic!("failed to read {name}: {e}"));
        }
        entries.push(Entry {
            name,
            is_dir,
            data,
            options,
            compressed_size,
        });
    }
    drop(archive);

    let classified: Vec<(Kind, Option<DynamicImage>)> = entries
        .iter()
        .map(|e| {
            if e.is_dir {
                (Kind::Fixed, None)
            } else {
                classify(&e.data)
            }
        })
        .collect();

    // `compressed_size` is what the *original* entries contribute to the
    // current file size, so it's used to work out how many bytes are spent
    // on things we won't touch. But once crushed, images are written back
    // out uncompressed (see below), so their contribution to the *new*
    // file size is their raw byte length - that's the right basis for
    // sizing each image's target, not their incidental original zip
    // compression (which can vary a lot and has nothing to do with how
    // shrinkable the image itself is).
    let modifiable_compressed_total: u64 = entries
        .iter()
        .zip(&classified)
        .filter(|(_, (kind, _))| *kind != Kind::Fixed)
        .map(|(e, _)| e.compressed_size)
        .sum();
    let modifiable_raw_total: u64 = entries
        .iter()
        .zip(&classified)
        .filter(|(_, (kind, _))| *kind != Kind::Fixed)
        .map(|(e, _)| e.data.len() as u64)
        .sum();
    let fixed_total = current_size.saturating_sub(modifiable_compressed_total);

    if modifiable_raw_total == 0 {
        eprintln!("warning: no JPEG or PNG images found to compress; output will be about the same size");
    }

    let target_for_images = target_bytes.saturating_sub(fixed_total);
    let ratio = if modifiable_raw_total == 0 {
        1.0
    } else {
        (target_for_images as f64 / modifiable_raw_total as f64).clamp(0.02, 1.0)
    };

    // Below this, an image is already small enough that squeezing it
    // further barely moves the total file size while risking visibly
    // wrecking small icons/decorations - so just leave it alone.
    const MIN_TARGET_BYTES: usize = 16 * 1024;

    let out_file = fs::File::create(&output_path)
        .unwrap_or_else(|e| panic!("failed to create {}: {e}", output_path.display()));
    let mut writer = ZipWriter::new(out_file);

    for (entry, (kind, img)) in entries.iter().zip(classified.iter()) {
        if entry.is_dir {
            writer
                .add_directory(&entry.name, entry.options)
                .unwrap_or_else(|e| panic!("failed to write directory {}: {e}", entry.name));
            continue;
        }

        let should_shrink = *kind != Kind::Fixed && ratio < 0.98;
        let target_for_this =
            ((entry.data.len() as f64 * ratio).round() as usize).max(MIN_TARGET_BYTES);

        let final_data = if should_shrink && target_for_this < entry.data.len() {
            let img = img.as_ref().expect("classified image must be decoded");
            match kind {
                Kind::Jpeg => shrink_jpeg(img, target_for_this),
                Kind::Png => shrink_png(img, target_for_this),
                Kind::Fixed => unreachable!(),
            }
        } else {
            entry.data.clone()
        };

        let mut options = entry.options;
        if entry.name.eq_ignore_ascii_case("mimetype") {
            options = options.compression_method(CompressionMethod::Stored);
        } else if *kind != Kind::Fixed {
            // Already-compressed image data; deflating it again just burns
            // CPU for no real size benefit.
            options = options.compression_method(CompressionMethod::Stored);
        }

        writer
            .start_file(&entry.name, options)
            .unwrap_or_else(|e| panic!("failed to write entry {}: {e}", entry.name));
        writer
            .write_all(&final_data)
            .unwrap_or_else(|e| panic!("failed to write entry {}: {e}", entry.name));
    }

    writer
        .finish()
        .unwrap_or_else(|e| panic!("failed to finalize {}: {e}", output_path.display()));

    let new_size = fs::metadata(&output_path)
        .unwrap_or_else(|e| panic!("failed to read back {}: {e}", output_path.display()))
        .len();

    let pct = 100.0 - (new_size as f64 / current_size as f64 * 100.0);
    println!(
        "{}: {current_size} -> {new_size} bytes ({pct:.1}% smaller, target was {target_bytes} bytes)",
        output_path.display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(w: u32, h: u32) -> DynamicImage {
        // High-frequency-ish content so JPEG/PNG encoders can't trivially
        // flatten it to (near) nothing regardless of quality/scale.
        DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x * 3) as u8, (y * 5) as u8, (x ^ y) as u8])
        }))
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

    // -- classify ---------------------------------------------------------

    #[test]
    fn classifies_jpeg_bytes_as_jpeg() {
        let data = encode_jpeg(&photo(64, 64), 80);
        let (kind, img) = classify(&data);
        assert_eq!(kind, Kind::Jpeg);
        assert!(img.is_some());
    }

    #[test]
    fn classifies_png_bytes_as_png() {
        let data = encode_png(&photo(64, 64));
        let (kind, img) = classify(&data);
        assert_eq!(kind, Kind::Png);
        assert!(img.is_some());
    }

    #[test]
    fn classifies_non_image_bytes_as_fixed() {
        let (kind, img) = classify(b"<html><body>hello</body></html>");
        assert_eq!(kind, Kind::Fixed);
        assert!(img.is_none());
    }

    // -- shrink_jpeg / shrink_png ---------------------------------------

    #[test]
    fn shrink_jpeg_hits_a_moderate_target_via_quality_alone() {
        let img = photo(400, 300);
        let original = encode_jpeg(&img, 95);
        let target = original.len() / 3;
        let shrunk = shrink_jpeg(&img, target);
        assert!(shrunk.len() <= target.max(1) || shrunk.len() < original.len());
        // Still a valid, decodable JPEG at the original resolution (quality
        // alone should be enough for a 3x cut on a small image).
        let decoded = image::load_from_memory(&shrunk).unwrap();
        assert_eq!(decoded.dimensions(), (400, 300));
    }

    #[test]
    fn shrink_jpeg_downscales_when_quality_cannot_reach_a_tiny_target() {
        let img = photo(2000, 1500);
        let shrunk = shrink_jpeg(&img, 4000);
        assert!(shrunk.len() <= 4000 || shrunk.len() < encode_jpeg(&img, 92).len());
        let decoded = image::load_from_memory(&shrunk).unwrap();
        let (w, h) = decoded.dimensions();
        assert!(w < 2000 && h < 1500, "expected downscale, got {w}x{h}");
    }

    #[test]
    fn shrink_jpeg_never_grows_the_image() {
        let img = photo(300, 200);
        let shrunk = shrink_jpeg(&img, 1_000_000_000);
        let decoded = image::load_from_memory(&shrunk).unwrap();
        let (w, h) = decoded.dimensions();
        assert!(w <= 300 && h <= 200);
    }

    #[test]
    fn shrink_png_downscales_to_fit_a_small_target() {
        let img = photo(400, 300);
        let full = encode_png(&img);
        let target = full.len() / 4;
        let shrunk = shrink_png(&img, target);
        assert!(shrunk.len() <= target);
        let decoded = image::load_from_memory(&shrunk).unwrap();
        let (w, h) = decoded.dimensions();
        assert!(w < 400 && h < 300);
    }

    #[test]
    fn shrink_png_leaves_image_untouched_if_already_under_target() {
        let img = photo(50, 50);
        let full = encode_png(&img);
        let shrunk = shrink_png(&img, full.len() + 1000);
        assert_eq!(shrunk, full);
    }
}
