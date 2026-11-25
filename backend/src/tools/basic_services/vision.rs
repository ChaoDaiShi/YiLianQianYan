use base64::{engine::general_purpose::STANDARD, Engine};
use image::{
    codecs::jpeg::JpegEncoder, imageops::FilterType, DynamicImage, ImageFormat, Rgb, RgbImage,
};
use std::io::Cursor;

const VISION_MAX_EDGE: u32 = 4096;
const VISION_MAX_BYTES: usize = 6 * 1024 * 1024;

pub(super) fn resource_data_url(mime: &str, bytes: Vec<u8>) -> Result<String, String> {
    let image = crate::modules::resource::parsers::image::decode(mime, &bytes)?;
    if bytes.len() <= VISION_MAX_BYTES && image.width().max(image.height()) <= VISION_MAX_EDGE {
        return Ok(data_url(mime, &bytes));
    }
    let image = if image.width().max(image.height()) > VISION_MAX_EDGE {
        image.resize(VISION_MAX_EDGE, VISION_MAX_EDGE, FilterType::Triangle)
    } else {
        image
    };
    if mime == "image/png" {
        let mut encoded = Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, ImageFormat::Png)
            .map_err(|_| "无法准备识图副本")?;
        if encoded.get_ref().len() <= VISION_MAX_BYTES {
            return Ok(data_url(mime, encoded.get_ref()));
        }
    }
    let mut encoded = jpeg_on_white(&image)?;
    if encoded.len() > VISION_MAX_BYTES && image.width().max(image.height()) > 2048 {
        let reduced = image.resize(2048, 2048, FilterType::Triangle);
        encoded = jpeg_on_white(&reduced)?;
    }
    if encoded.len() > VISION_MAX_BYTES {
        return Err("图片副本仍超过识图请求大小限制".into());
    }
    Ok(data_url("image/jpeg", &encoded))
}

fn jpeg_on_white(image: &DynamicImage) -> Result<Vec<u8>, String> {
    let rgb = if image.color().has_alpha() {
        let rgba = image.to_rgba8();
        RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
            let [r, g, b, alpha] = rgba.get_pixel(x, y).0;
            let alpha = u16::from(alpha);
            let blend = |value: u8| ((u16::from(value) * alpha + 255 * (255 - alpha)) / 255) as u8;
            Rgb([blend(r), blend(g), blend(b)])
        })
    } else {
        image.to_rgb8()
    };
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 85)
        .encode_image(&rgb)
        .map_err(|_| "无法准备识图副本")?;
    Ok(bytes)
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dense_png_is_reencoded_without_enlarging_the_image() {
        let mut seed = 0x12345678_u32;
        let noise = RgbImage::from_fn(1800, 1800, |_, _| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            Rgb([seed as u8, (seed >> 8) as u8, (seed >> 16) as u8])
        });
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(noise)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        assert!(png.get_ref().len() > VISION_MAX_BYTES);

        let url = resource_data_url("image/png", png.into_inner()).unwrap();
        let jpeg = STANDARD
            .decode(url.strip_prefix("data:image/jpeg;base64,").unwrap())
            .unwrap();
        assert!(jpeg.len() <= VISION_MAX_BYTES);
        let decoded = image::load_from_memory_with_format(&jpeg, ImageFormat::Jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1800, 1800));
    }
}
