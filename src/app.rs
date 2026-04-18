use std::sync::Arc;
use tokio::sync::{Mutex, watch};

use crate::single_instance::SessionHandle;

/// Top-level application state. Owns the channel that coordinates
/// hotkey triggers and D-Bus activation into a single signal.
pub struct App {
    /// Receives trigger signals (Pause key or D-Bus activate).
    trigger_rx: watch::Receiver<bool>,
    /// D-Bus connection for KWin calls.
    dbus_conn: zbus::Connection,
    /// Slot for the oneshot sender that receives window data from KWin scripts.
    window_data_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
    /// If true, exit after the first session completes.
    single_session: bool,
}

impl App {
    /// Create a new App and a SessionHandle for the D-Bus service.
    pub fn new(dbus_conn: zbus::Connection, single_session: bool) -> (Self, SessionHandle) {
        let (activate_tx, trigger_rx) = watch::channel(false);
        let window_data_tx = Arc::new(Mutex::new(None));

        let app = App {
            trigger_rx,
            dbus_conn,
            window_data_tx: window_data_tx.clone(),
            single_session,
        };

        let handle = SessionHandle {
            activate_tx,
            window_data_tx,
        };

        (app, handle)
    }

    /// Run the main event loop. Waits for trigger signals and starts
    /// screenshot sessions.
    pub async fn run(mut self) -> anyhow::Result<()> {
        tracing::info!("app running, waiting for trigger...");

        loop {
            // Wait for the trigger channel to change.
            self.trigger_rx.changed().await?;

            tracing::info!("trigger received — starting screenshot session");

            if let Err(e) = self.start_session().await {
                tracing::error!("session failed: {e:#}");
            }

            if self.single_session {
                tracing::info!("single-session mode, exiting");
                return Ok(());
            }
        }
    }

    /// Show the overlay. Capture is done inside the overlay after it
    /// enumerates Wayland outputs, so each screen gets its own capture.
    async fn start_session(&self) -> anyhow::Result<()> {
        tracing::info!("starting overlay");

        // Fetch window list before entering the blocking overlay loop,
        // so the tokio runtime can still process D-Bus callbacks.
        let windows = crate::kwin::windows::fetch_window_list(
            &self.dbus_conn,
            &self.window_data_tx,
            crate::kwin::windows::DEFAULT_SERVICE_NAME,
        ).await.unwrap_or_else(|e| {
            tracing::warn!("failed to fetch window list, snap disabled: {e:#}");
            Vec::new()
        });

        crate::overlay::run(self.dbus_conn.clone(), windows)?;
        tracing::info!("overlay closed");
        Ok(())
    }
}
