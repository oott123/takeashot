/// KWin scripting integration for window list retrieval.
/// Will be implemented in M6.

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct WindowInfo {
    pub title: String,
    pub resource_class: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Retrieve the list of normal, non-minimized windows via KWin scripting.
///
/// For M2 this returns an empty list. Full implementation in M6.
pub async fn get_window_list(
    _conn: &zbus::Connection,
    _window_data_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<Vec<WindowInfo>> {
    tracing::debug!("window listing not yet implemented, returning empty list");
    Ok(Vec::new())
}
