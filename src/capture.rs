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

/// Output geometry needed for cropping a workspace capture to per-output screens.
pub struct OutputCaptureInfo {
    pub name: String,
    pub logical_x: i32,
    pub logical_y: i32,
    pub logical_w: i32,
    pub logical_h: i32,
    pub scale_factor: i32,
}

/// Capture each named screen individually using KWin's CaptureScreen.
/// Falls back to a single workspace capture (cropped per output) if per-screen fails.
/// When `force_workspace` is true, skips per-screen capture entirely.
pub async fn capture_all(
    conn: &zbus::Connection,
    output_infos: &[OutputCaptureInfo],
    force_workspace: bool,
) -> Result<Vec<CapturedScreen>> {
    if force_workspace || output_infos.is_empty() {
        if output_infos.is_empty() {
            tracing::warn!("no output info provided, falling back to workspace capture");
        } else {
            tracing::warn!("--use-workspace: forcing workspace capture instead of per-screen capture");
        }
        return capture_workspace_fallback(conn, output_infos).await;
    }

    let mut screens = Vec::with_capacity(output_infos.len());
    for info in output_infos {
        match screenshot::capture_screen(conn, &info.name).await {
            Ok((bgra, meta)) => {
                tracing::info!("captured screen '{}': {}x{}, stride={}", info.name, meta.width, meta.height, meta.stride);
                screens.push(CapturedScreen {
                    name: info.name.clone(),
                    bgra,
                    width: meta.width,
                    height: meta.height,
                    stride: meta.stride,
                });
            }
            Err(e) => {
                tracing::warn!("CaptureScreen('{}') failed: {e:#}, falling back to workspace capture — each screen will show a cropped region of the full workspace", info.name);
                return capture_workspace_fallback(conn, output_infos).await;
            }
        }
    }

    if screens.is_empty() {
        anyhow::bail!("no screens captured");
    }
    Ok(screens)
}

async fn capture_workspace_fallback(
    conn: &zbus::Connection,
    output_infos: &[OutputCaptureInfo],
) -> Result<Vec<CapturedScreen>> {
    let (bgra, meta) = screenshot::capture_workspace(conn)
        .await
        .context("workspace capture failed")?;

    tracing::info!(
        "captured workspace: {}x{}, stride={}, format={}",
        meta.width, meta.height, meta.stride, meta.format
    );

    if output_infos.is_empty() {
        return Ok(vec![CapturedScreen {
            name: "workspace".to_owned(),
            bgra,
            width: meta.width,
            height: meta.height,
            stride: meta.stride,
        }]);
    }

    // Crop the workspace image to each output's region.
    output_infos
        .iter()
        .map(|info| crop_workspace_to_output(&bgra, &meta, info))
        .collect()
}

fn crop_workspace_to_output(
    workspace_bgra: &[u8],
    workspace_meta: &screenshot::CaptureMetadata,
    output: &OutputCaptureInfo,
) -> Result<CapturedScreen> {
    let scale = output.scale_factor.max(1) as u32;
    let px = (output.logical_x as u32) * scale;
    let py = (output.logical_y as u32) * scale;
    let pw = (output.logical_w as u32) * scale;
    let ph = (output.logical_h as u32) * scale;

    if px + pw > workspace_meta.width || py + ph > workspace_meta.height {
        anyhow::bail!(
            "output '{}' region ({px},{py}+{pw}x{ph}) exceeds workspace ({}x{})",
            output.name, workspace_meta.width, workspace_meta.height
        );
    }

    let src_stride = workspace_meta.stride as usize;
    let dst_stride = pw as usize * 4;
    let mut cropped = Vec::with_capacity(ph as usize * dst_stride);

    for y in 0..ph {
        let src_offset = (py + y) as usize * src_stride + px as usize * 4;
        let src_end = src_offset + dst_stride;
        if src_end > workspace_bgra.len() {
            anyhow::bail!("workspace data truncated at row {}", py + y);
        }
        cropped.extend_from_slice(&workspace_bgra[src_offset..src_end]);
    }

    tracing::info!(
        "cropped workspace to output '{}': ({px},{py}) {pw}x{ph}",
        output.name
    );

    Ok(CapturedScreen {
        name: output.name.clone(),
        bgra: cropped,
        width: pw,
        height: ph,
        stride: pw * 4,
    })
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
