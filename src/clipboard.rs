use anyhow::Result;
use image::RgbaImage;

/// Encode an image as PNG and copy to the Wayland clipboard with `image/png` MIME type.
pub fn copy_to_clipboard(img: RgbaImage) -> Result<()> {
    let mut png_buf = Vec::with_capacity((img.width() * img.height() * 4) as usize);
    let encoder = image::codecs::png::PngEncoder::new(&mut png_buf);
    image::ImageEncoder::write_image(
        encoder,
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    )?;

    use wl_clipboard_rs::copy::{MimeType, Options, Source};
    let opts = Options::new();
    let src = Source::Bytes(png_buf.into());
    opts.copy(src, MimeType::Specific("image/png".to_owned()))
        .map_err(|e| anyhow::anyhow!("wl-clipboard copy failed: {e}"))?;
    Ok(())
}
