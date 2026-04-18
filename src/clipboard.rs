use anyhow::Result;

/// Copy PNG bytes to the Wayland clipboard with `image/png` MIME type.
pub fn copy_to_clipboard(png_bytes: &[u8]) -> Result<()> {
    use wl_clipboard_rs::copy::{MimeType, Options, Source};

    let opts = Options::new();
    let src = Source::Bytes(png_bytes.into());
    opts.copy(src, MimeType::Specific("image/png".to_owned()))
        .map_err(|e| anyhow::anyhow!("wl-clipboard copy failed: {e}"))?;
    Ok(())
}
