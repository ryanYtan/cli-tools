//! Test images shared by the unit tests and the CLI tests.

use image::RgbImage;

/// An image with high-frequency content, so JPEG/PNG encoders can't flatten
/// it to (near) nothing regardless of quality/scale.
pub fn photo(w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 3) as u8, (y * 5) as u8, (x ^ y) as u8]))
}
