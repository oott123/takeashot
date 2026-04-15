use anyhow::{Context, Result};
use std::os::fd::AsRawFd;
use zbus::zvariant::{OwnedFd, OwnedValue, Value};

/// Metadata returned by KWin ScreenShot2 after a capture.
#[derive(Debug)]
pub struct CaptureMetadata {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: u32,
}

/// Duplicate a raw fd into an `OwnedFd` suitable for D-Bus transfer.
unsafe fn dup_to_owned_fd(raw_fd: i32) -> OwnedFd {
    let duped = unsafe { libc::dup(raw_fd) };
    assert!(duped >= 0, "dup() failed");
    let os_owned = unsafe { std::os::fd::OwnedFd::from_raw_fd(duped) };
    OwnedFd::from(os_owned)
}

// Helper: bring FromRawFd in scope for the above.
use std::os::fd::FromRawFd;

/// Capture the entire workspace using `CaptureWorkspace`.
///
/// Returns raw BGRA pixel data and metadata.
pub async fn capture_workspace(
    conn: &zbus::Connection,
) -> Result<(Vec<u8>, CaptureMetadata)> {
    let file = tempfile::tempfile().context("failed to create tempfile")?;
    let dbus_fd = unsafe { dup_to_owned_fd(file.as_raw_fd()) };

    let options: std::collections::HashMap<&str, Value<'_>> =
        std::collections::HashMap::from([("native-resolution", Value::Bool(true))]);

    let reply = conn
        .call_method(
            Some("org.kde.KWin"),
            "/org/kde/KWin/ScreenShot2",
            Some("org.kde.KWin.ScreenShot2"),
            "CaptureWorkspace",
            &(options, dbus_fd),
        )
        .await
        .context("CaptureWorkspace D-Bus call failed")?;

    let metadata_val: std::collections::HashMap<String, OwnedValue> = reply
        .body()
        .deserialize()
        .context("failed to deserialize CaptureWorkspace metadata")?;

    let metadata = parse_metadata(&metadata_val)?;
    let data = read_bgra_data(file, &metadata)?;

    Ok((data, metadata))
}

/// Capture a single screen by name using `CaptureScreen`.
pub async fn capture_screen(
    conn: &zbus::Connection,
    screen_name: &str,
) -> Result<(Vec<u8>, CaptureMetadata)> {
    let file = tempfile::tempfile().context("failed to create tempfile")?;
    let dbus_fd = unsafe { dup_to_owned_fd(file.as_raw_fd()) };

    let options: std::collections::HashMap<&str, Value<'_>> =
        std::collections::HashMap::from([("native-resolution", Value::Bool(true))]);

    let reply = conn
        .call_method(
            Some("org.kde.KWin"),
            "/org/kde/KWin/ScreenShot2",
            Some("org.kde.KWin.ScreenShot2"),
            "CaptureScreen",
            &(screen_name, options, dbus_fd),
        )
        .await
        .context(format!("CaptureScreen({screen_name}) D-Bus call failed"))?;

    let metadata_val: std::collections::HashMap<String, OwnedValue> = reply
        .body()
        .deserialize()
        .context("failed to deserialize CaptureScreen metadata")?;

    let metadata = parse_metadata(&metadata_val)?;
    let data = read_bgra_data(file, &metadata)?;

    Ok((data, metadata))
}

fn extract_u32(val: &OwnedValue) -> Result<u32> {
    match <&Value<'_>>::from(val) {
        Value::U32(v) => Ok(*v),
        Value::I64(v) => Ok(*v as u32),
        Value::U64(v) => Ok(*v as u32),
        other => anyhow::bail!("unexpected integer type: {other:?}"),
    }
}

fn parse_metadata(raw: &std::collections::HashMap<String, OwnedValue>) -> Result<CaptureMetadata> {
    let width = raw
        .get("width")
        .ok_or_else(|| anyhow::anyhow!("missing width in metadata"))
        .and_then(extract_u32)?;
    let height = raw
        .get("height")
        .ok_or_else(|| anyhow::anyhow!("missing height in metadata"))
        .and_then(extract_u32)?;
    let stride = raw
        .get("stride")
        .ok_or_else(|| anyhow::anyhow!("missing stride in metadata"))
        .and_then(extract_u32)?;
    let format = raw
        .get("format")
        .ok_or_else(|| anyhow::anyhow!("missing format in metadata"))
        .and_then(extract_u32)?;

    Ok(CaptureMetadata {
        width,
        height,
        stride,
        format,
    })
}

/// Read BGRA data from the file that KWin wrote into.
fn read_bgra_data(mut file: std::fs::File, meta: &CaptureMetadata) -> Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    file.seek(SeekFrom::Start(0))?;
    let expected_size = meta.stride as usize * meta.height as usize;
    let mut buf = Vec::with_capacity(expected_size);
    file.read_to_end(&mut buf)?;

    if buf.is_empty() {
        anyhow::bail!("KWin wrote zero bytes to screenshot fd");
    }

    tracing::debug!(
        "read {} bytes from screenshot fd (expected {expected_size})",
        buf.len()
    );

    Ok(buf)
}
