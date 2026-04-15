use anyhow::{Context, Result};
use image::RgbaImage;

use crate::kwin::screenshot;

/// A captured screen: raw BGRA data + metadata.
pub struct CapturedScreen {
    pub name: String,
    pub bgra: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

/// Capture each named screen individually using KWin's CaptureScreen.
/// Falls back to a single workspace capture if per-screen fails.
pub async fn capture_all(conn: &zbus::Connection, output_names: &[String]) -> Result<Vec<CapturedScreen>> {
    if output_names.is_empty() {
        tracing::warn!("no output names provided, falling back to workspace capture");
        return capture_workspace_fallback(conn).await;
    }

    let mut screens = Vec::with_capacity(output_names.len());
    for name in output_names {
        match screenshot::capture_screen(conn, name).await {
            Ok((bgra, meta)) => {
                tracing::info!("captured screen '{name}': {}x{}, stride={}", meta.width, meta.height, meta.stride);
                screens.push(CapturedScreen {
                    name: name.clone(),
                    bgra,
                    width: meta.width,
                    height: meta.height,
                    stride: meta.stride,
                });
            }
            Err(e) => {
                tracing::warn!("CaptureScreen('{name}') failed: {e:#}, trying workspace fallback");
                return capture_workspace_fallback(conn).await;
            }
        }
    }

    if screens.is_empty() {
        anyhow::bail!("no screens captured");
    }
    Ok(screens)
}

async fn capture_workspace_fallback(conn: &zbus::Connection) -> Result<Vec<CapturedScreen>> {
    let (bgra, meta) = screenshot::capture_workspace(conn)
        .await
        .context("workspace capture failed")?;

    tracing::info!(
        "captured workspace: {}x{}, stride={}, format={}",
        meta.width, meta.height, meta.stride, meta.format
    );

    Ok(vec![CapturedScreen {
        name: "workspace".to_owned(),
        bgra,
        width: meta.width,
        height: meta.height,
        stride: meta.stride,
    }])
}

/// Convert BGRA raw data to an `image::RgbaImage` (RGBA).
pub fn bgra_to_rgba(captured: &CapturedScreen) -> Result<RgbaImage> {
    let w = captured.width;
    let h = captured.height;
    let stride = captured.stride as usize;
    let bpp = 4usize;

    if stride < w as usize * bpp {
        anyhow::bail!("stride {} too small for width {w} * {bpp} bpp", stride);
    }

    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let row_start = y as usize * stride;
        let row_end = row_start + w as usize * bpp;
        if row_end > captured.bgra.len() {
            anyhow::bail!("BGRA data truncated at row {y}");
        }
        for px in (row_start..row_end).step_by(4) {
            let b = captured.bgra[px];
            let g = captured.bgra[px + 1];
            let r = captured.bgra[px + 2];
            let a = captured.bgra[px + 3];
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }

    RgbaImage::from_raw(w, h, rgba).context("failed to create RgbaImage from converted data")
}

/// Save a captured screen as PNG to the given path.
pub fn save_png(captured: &CapturedScreen, path: &std::path::Path) -> Result<()> {
    let img = bgra_to_rgba(captured)?;
    img.save(path)
        .with_context(|| format!("failed to save PNG to {}", path.display()))?;
    tracing::info!("saved screenshot to {}", path.display());
    Ok(())
}
