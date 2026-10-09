//! Bounded image decoding.
//!
//! Images contribute dimensions, not text — no OCR runs here. Decoding is
//! capped by pixel count and allocation so a decompression bomb cannot claim
//! memory, and the format is pinned from the canonical MIME rather than
//! sniffed, so a file cannot pick its own decoder.

pub(crate) fn dimensions(mime: &str, bytes: &[u8]) -> Result<(u32, u32), String> {
    let expected = if mime == "image/png" {
        image::ImageFormat::Png
    } else {
        image::ImageFormat::Jpeg
    };
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), expected);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map(|image| (image.width(), image.height()))
        .map_err(|_| "图片解码失败或超过像素限制".into())
}
