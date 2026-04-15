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
}

impl App {
    /// Create a new App and a SessionHandle for the D-Bus service.
    pub fn new(dbus_conn: zbus::Connection) -> (Self, SessionHandle) {
        let (activate_tx, trigger_rx) = watch::channel(false);
        let window_data_tx = Arc::new(Mutex::new(None));

        let app = App {
            trigger_rx,
            dbus_conn,
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
        }
    }

    /// Show the overlay. Capture is done inside the overlay after it
    /// enumerates Wayland outputs, so each screen gets its own capture.
    async fn start_session(&self) -> anyhow::Result<()> {
        tracing::info!("starting overlay");
        crate::overlay::run(self.dbus_conn.clone())?;
        tracing::info!("overlay closed");
        Ok(())
    }
}
