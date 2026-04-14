use anyhow::Result;
use evdev::{Device, EventSummary, KeyCode};
use tokio::sync::watch;

/// Scan `/dev/input/` for keyboard devices and monitor for KEY_PAUSE.
///
/// When the Pause key is pressed, sends `true` on `trigger_tx`.
/// Errors opening individual devices are logged and skipped — the task
/// never crashes.
pub async fn listen(trigger_tx: watch::Sender<bool>) {
    loop {
        if let Err(e) = listen_inner(&trigger_tx).await {
            tracing::error!("hotkey listener failed, restarting in 2s: {e}");
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }
}

async fn listen_inner(trigger_tx: &watch::Sender<bool>) -> Result<()> {
    let devices = discover_keyboards()?;
    if devices.is_empty() {
        tracing::warn!("no keyboard devices found in /dev/input/");
    }

    let mut streams: Vec<_> = Vec::new();

    for device in devices {
        let dev_name = device.name().unwrap_or("?").to_owned();
        match device.into_event_stream() {
            Ok(stream) => {
                tracing::debug!("monitoring keyboard: {dev_name}");
                streams.push(stream);
            }
            Err(e) => {
                tracing::warn!("failed to open {dev_name} as event stream: {e}");
            }
        }
    }

    if streams.is_empty() {
        anyhow::bail!("no keyboard event streams available");
    }

    // Monitor all streams concurrently using tokio::select! style.
    // We spawn one task per stream that forwards Pause key presses.
    let mut handles = Vec::new();
    for mut stream in streams {
        let tx = trigger_tx.clone();
        handles.push(tokio::spawn(async move {
            loop {
                match stream.next_event().await {
                    Ok(event) => {
                        if let EventSummary::Key(_, KeyCode::KEY_PAUSE, value) =
                            event.destructure()
                        {
                            if value == 1 {
                                tracing::info!("Pause key pressed");
                                let _ = tx.send(true);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("keyboard stream error: {e}");
                        break;
                    }
                }
            }
        }));
    }

    // Wait for all streams to end (they shouldn't unless device disconnects).
    for handle in handles {
        let _ = handle.await;
    }

    tracing::warn!("all keyboard event streams ended");
    Ok(())
}

/// Enumerate `/dev/input/event*` and return devices whose name contains "keyboard".
fn discover_keyboards() -> Result<Vec<Device>> {
    let mut keyboards = Vec::new();

    for entry in std::fs::read_dir("/dev/input")? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if !name.starts_with("event") {
            continue;
        }

        let path = entry.path();
        match Device::open(&path) {
            Ok(device) => {
                let dev_name = device.name().unwrap_or("").to_lowercase();
                if dev_name.contains("keyboard") {
                    tracing::debug!("found keyboard device: {path:?} ({dev_name})");
                    keyboards.push(device);
                }
            }
            Err(e) => {
                tracing::debug!("cannot open {path:?}: {e}");
            }
        }
    }

    Ok(keyboards)
}
