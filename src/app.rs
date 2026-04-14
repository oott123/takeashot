use std::sync::Arc;
use tokio::sync::{Mutex, watch};

use crate::single_instance::SessionHandle;

/// Top-level application state. Owns the channel that coordinates
/// hotkey triggers and D-Bus activation into a single signal.
pub struct App {
    /// Receives trigger signals (Pause key or D-Bus activate).
    trigger_rx: watch::Receiver<bool>,
}

impl App {
    /// Create a new App and a SessionHandle for the D-Bus service.
    pub fn new() -> (Self, SessionHandle) {
        let (activate_tx, trigger_rx) = watch::channel(false);
        let window_data_tx = Arc::new(Mutex::new(None));

        let app = App { trigger_rx };
        let handle = SessionHandle {
            activate_tx,
            window_data_tx,
        };

        (app, handle)
    }

    /// Run the main event loop. Waits for trigger signals and starts
    /// screenshot sessions. For M1, we just log and acknowledge.
    pub async fn run(mut self) -> anyhow::Result<()> {
        tracing::info!("app running, waiting for trigger...");

        loop {
            // Wait for the trigger channel to change.
            self.trigger_rx.changed().await?;

            tracing::info!("trigger received — starting screenshot session (M1 stub)");

            // M1: just acknowledge. Future milestones will launch ShotSession here.
        }
    }
}
