/// KWin scripting integration for window list retrieval.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};

#[derive(Debug, Deserialize, Clone)]
pub struct WindowInfo {
    pub caption: String,
    #[serde(rename = "resourceClass")]
    pub resource_class: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

const SCRIPT_TEMPLATE: &str = include_str!("window_script.js");
const PLUGIN_NAME: &str = "takeashot-window-list";

/// Default D-Bus service name used by the main instance.
pub const DEFAULT_SERVICE_NAME: &str = "com.takeashot.service";

/// Retrieve the list of normal, non-minimized windows via KWin scripting.
///
/// `service_name` is the D-Bus service name where `receive_window_data` is
/// registered. For the main instance this is `com.takeashot.service`; for
/// smoke mode it's a random name to avoid conflicts.
///
/// The pipeline:
/// 1. Create a oneshot channel and install the sender into `window_data_slot`
/// 2. Write the JS script (with the service name baked in) to a temp file
/// 3. Call `loadScript` → `run()` on KWin Scripting D-Bus
/// 4. Wait for the `receive_window_data` callback (5s timeout)
/// 5. Unload the script and delete the temp file
/// 6. Parse the JSON into `Vec<WindowInfo>`
///
/// On failure or timeout, returns an empty list (snap is silently disabled).
pub async fn fetch_window_list(
    conn: &zbus::Connection,
    window_data_slot: &Arc<Mutex<Option<oneshot::Sender<String>>>>,
    service_name: &str,
) -> Result<Vec<WindowInfo>> {
    // 1. Set up oneshot channel
    let (tx, rx) = oneshot::channel::<String>();
    {
        let mut guard = window_data_slot.lock().await;
        *guard = Some(tx);
    }

    // 2. Write script to temp file (use current dir — /tmp may not be
    //    shared with KWin when running in a sandboxed environment).
    //    Must be an absolute path for KWin to find it.
    let script_path = std::env::current_dir()
        .context("failed to get current directory")?
        .join(format!(".takeashot-script-{:x}.js", std::process::id()));

    // Inject the service name into the script template
    let script_content = SCRIPT_TEMPLATE.replace("{{SERVICE_NAME}}", service_name);
    {
        let mut f = std::fs::File::create(&script_path)
            .context("failed to create temp script file")?;
        f.write_all(script_content.as_bytes())
            .context("failed to write script")?;
    }
    tracing::debug!("wrote KWin script to {}", script_path.display());

    // Ensure cleanup on all exit paths
    let cleanup = Cleanup {
        conn: conn.clone(),
        path: script_path.clone(),
    };

    let result = fetch_window_list_inner(conn, rx, &script_path).await;

    // 5-6. Cleanup: unload script + delete temp file
    cleanup.run().await;

    result
}

struct Cleanup {
    conn: zbus::Connection,
    path: std::path::PathBuf,
}

impl Cleanup {
    async fn run(&self) {
        // Unload script
        let reply = self.conn
            .call_method(
                Some("org.kde.KWin"),
                "/Scripting",
                Some("org.kde.kwin.Scripting"),
                "unloadScript",
                &("takeashot-window-list"),
            )
            .await;
        if let Err(e) = reply {
            tracing::debug!("unloadScript failed (non-fatal): {e}");
        }

        // Delete temp file
        if let Err(e) = std::fs::remove_file(&self.path) {
            tracing::debug!("failed to delete temp script: {e}");
        }
    }
}

async fn fetch_window_list_inner(
    conn: &zbus::Connection,
    rx: oneshot::Receiver<String>,
    script_path: &std::path::Path,
) -> Result<Vec<WindowInfo>> {
    let path_str = script_path.to_str()
        .ok_or_else(|| anyhow::anyhow!("script path is not valid UTF-8"))?;

    // 3. loadScript
    let reply = conn
        .call_method(
            Some("org.kde.KWin"),
            "/Scripting",
            Some("org.kde.kwin.Scripting"),
            "loadScript",
            &(path_str, PLUGIN_NAME),
        )
        .await
        .context("loadScript D-Bus call failed")?;

    let script_id: i32 = reply.body().deserialize()
        .context("failed to deserialize loadScript reply")?;
    tracing::debug!("KWin script loaded, id={script_id}");

    if script_id < 0 {
        anyhow::bail!("KWin loadScript returned id={script_id}, file may not be readable by KWin (path={path_str})");
    }

    // 4. Run the script
    let script_object_path = format!("/Scripting/Script{script_id}");
    conn.call_method(
        Some("org.kde.KWin"),
        script_object_path.as_str(),
        Some("org.kde.kwin.Script"),
        "run",
        &(),
    )
    .await
    .context("Script.run() D-Bus call failed")?;

    tracing::debug!("KWin script running, waiting for window data...");

    // 5. Wait for the callback with 5s timeout
    let json_str = match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        rx,
    ).await {
        Ok(Ok(json)) => json,
        Ok(Err(_)) => {
            tracing::warn!("window data oneshot sender was dropped");
            return Ok(Vec::new());
        }
        Err(_) => {
            tracing::warn!("window data timeout (5s), snap disabled");
            return Ok(Vec::new());
        }
    };

    // 6. Parse JSON
    let windows: Vec<WindowInfo> = match serde_json::from_str::<Vec<WindowInfo>>(&json_str) {
        Ok(w) => {
            tracing::info!("received {} windows from KWin", w.len());
            w
        }
        Err(e) => {
            tracing::warn!("failed to parse window data JSON: {e}");
            tracing::debug!("raw JSON (first 500 chars): {}", &json_str[..json_str.len().min(500)]);
            Vec::new()
        }
    };

    Ok(windows)
}
