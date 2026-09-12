use std::io::Write;
use std::path::Path;
use std::process::Command;

use image::codecs::jpeg::JpegEncoder;
use image::{ImageFormat, RgbImage};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// A large, high-frequency (so it doesn't trivially compress away) photo.
fn photo_jpeg_bytes(w: u32, h: u32, quality: u8) -> Vec<u8> {
    let img = RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 3) as u8, (y * 5) as u8, (x ^ y) as u8]));
    let mut buf = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, quality)
        .encode_image(&img)
        .unwrap();
    buf
}

/// Builds a minimal, valid EPUB at `path` with `n_photos` embedded JPEGs.
fn build_epub(path: &Path, n_photos: usize) {
    let file = std::fs::File::create(path).unwrap();
    let mut z = ZipWriter::new(file);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    z.start_file("mimetype", stored).unwrap();
    z.write_all(b"application/epub+zip").unwrap();

    z.start_file("META-INF/container.xml", deflated).unwrap();
    z.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();

    let mut manifest = String::new();
    for i in 0..n_photos {
        let name = format!("OEBPS/images/photo{i}.jpg");
        z.start_file(&name, deflated).unwrap();
        z.write_all(&photo_jpeg_bytes(800, 600, 92)).unwrap();
        manifest.push_str(&format!(
            r#"<item id="photo{i}" href="images/photo{i}.jpg" media-type="image/jpeg"/>"#
        ));
    }

    z.start_file("OEBPS/chapter0.xhtml", deflated).unwrap();
    z.write_all(b"<html><body><h1>Chapter</h1><p>hello world</p></body></html>")
        .unwrap();
    manifest.push_str(r#"<item id="ch0" href="chapter0.xhtml" media-type="application/xhtml+xml"/>"#);

    let opf = format!(
        r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="BookId">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Test</dc:title><dc:identifier id="BookId">id</dc:identifier><dc:language>en</dc:language></metadata>
<manifest>{manifest}<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/></manifest>
<spine toc="ncx"><itemref idref="ch0"/></spine></package>"#
    );
    z.start_file("OEBPS/content.opf", deflated).unwrap();
    z.write_all(opf.as_bytes()).unwrap();

    z.start_file("OEBPS/toc.ncx", deflated).unwrap();
    z.write_all(br#"<?xml version="1.0"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><navMap/></ncx>"#).unwrap();

    z.finish().unwrap();
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_epubc"))
        .args(args)
        .output()
        .expect("failed to run epubc")
}

#[test]
fn crushes_an_epub_below_its_target_size() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("book.epub");
    build_epub(&input, 3);
    let original_size = std::fs::metadata(&input).unwrap().len();

    let output = dir.path().join("out.epub");
    let result = run(&[
        input.to_str().unwrap(),
        "-t",
        "30%",
        "-o",
        output.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{result:?}");

    let new_size = std::fs::metadata(&output).unwrap().len();
    assert!(
        new_size < original_size,
        "expected shrink: {original_size} -> {new_size}"
    );
    // Should land reasonably close to the 30% target (allow slack for
    // small fixed content and per-image floors).
    assert!(new_size <= (original_size as f64 * 0.5) as u64);
}

#[test]
fn output_is_a_valid_zip_with_mimetype_first_and_stored() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("book.epub");
    build_epub(&input, 2);
    let output = dir.path().join("out.epub");

    let result = run(&[
        input.to_str().unwrap(),
        "-t",
        "20%",
        "-o",
        output.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{result:?}");

    let file = std::fs::File::open(&output).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    assert!(!archive.is_empty());
    let first = archive.by_index(0).unwrap();
    assert_eq!(first.name(), "mimetype");
    assert_eq!(first.compression(), CompressionMethod::Stored);
    drop(first);

    // Images should still be valid, decodable JPEGs.
    let mut found_image = false;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        if entry.name().ends_with(".jpg") {
            found_image = true;
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut data).unwrap();
            let img = image::load_from_memory_with_format(&data, ImageFormat::Jpeg).unwrap();
            use image::GenericImageView;
            let (w, h) = img.dimensions();
            assert!(w > 0 && h > 0);
        }
    }
    assert!(found_image);
}

#[test]
fn text_content_is_preserved_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("book.epub");
    build_epub(&input, 1);
    let output = dir.path().join("out.epub");

    let result = run(&[
        input.to_str().unwrap(),
        "-t",
        "10%",
        "-o",
        output.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{result:?}");

    let file = std::fs::File::open(&output).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let mut chapter = archive.by_name("OEBPS/chapter0.xhtml").unwrap();
    let mut contents = String::new();
    std::io::Read::read_to_string(&mut chapter, &mut contents).unwrap();
    assert_eq!(
        contents,
        "<html><body><h1>Chapter</h1><p>hello world</p></body></html>"
    );
}

#[test]
fn absolute_target_larger_than_current_size_is_a_no_op() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("book.epub");
    build_epub(&input, 1);
    let output = dir.path().join("out.epub");

    let result = run(&[
        input.to_str().unwrap(),
        "-t",
        "100MB",
        "-o",
        output.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{result:?}");
    assert!(!output.exists(), "no file should have been written");
}

#[test]
fn in_place_overwrites_the_input_file() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("book.epub");
    build_epub(&input, 2);
    let original_size = std::fs::metadata(&input).unwrap().len();

    let result = run(&[input.to_str().unwrap(), "-t", "30%", "--in-place"]);
    assert!(result.status.success(), "{result:?}");

    let new_size = std::fs::metadata(&input).unwrap().len();
    assert!(new_size < original_size);
}

#[test]
fn output_and_in_place_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("book.epub");
    build_epub(&input, 1);

    let result = run(&[
        input.to_str().unwrap(),
        "-t",
        "10%",
        "-o",
        "somewhere.epub",
        "--in-place",
    ]);
    assert!(!result.status.success());
}

#[test]
fn default_output_path_is_input_stem_with_crushed_suffix() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mybook.epub");
    build_epub(&input, 1);

    let result = run(&[input.to_str().unwrap(), "-t", "10%"]);
    assert!(result.status.success(), "{result:?}");
    assert!(dir.path().join("mybook.crushed.epub").exists());
}

#[test]
fn rejects_an_invalid_target_size() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("book.epub");
    build_epub(&input, 1);

    let result = run(&[input.to_str().unwrap(), "-t", "not-a-size"]);
    assert!(!result.status.success());
}
