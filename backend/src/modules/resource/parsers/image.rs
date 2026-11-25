//! Bounded image decoding.
//!
//! Images contribute dimensions, not text — no OCR runs here. Decoding is
//! capped by pixel count and allocation so a decompression bomb cannot claim
//! memory, and the format is pinned from the canonical MIME rather than
//! sniffed, so a file cannot pick its own decoder.

use image::ImageDecoder;

pub(crate) fn decode(mime: &str, bytes: &[u8]) -> Result<image::DynamicImage, String> {
    let expected = match mime {
        "image/png" => image::ImageFormat::Png,
        "image/jpeg" => image::ImageFormat::Jpeg,
        _ => return Err("不支持的图片格式".into()),
    };
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), expected);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(192 * 1024 * 1024);
    reader.limits(limits.clone());
    let decode = || -> image::ImageResult<_> {
        let mut decoder = reader.into_decoder()?;
        limits.reserve(decoder.total_bytes())?;
        decoder.set_limits(limits)?;
        let orientation = decoder
            .orientation()
            .unwrap_or(image::metadata::Orientation::NoTransforms);
        let mut image = image::DynamicImage::from_decoder(decoder)?;
        image.apply_orientation(orientation);
        Ok(image)
    };
    decode().map_err(|_| "图片解码失败或超过像素限制".into())
}

pub(crate) fn dimensions(mime: &str, bytes: &[u8]) -> Result<(u32, u32), String> {
    decode(mime, bytes).map(|image| (image.width(), image.height()))
}
