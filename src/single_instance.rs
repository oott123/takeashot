use anyhow::Result;
use zbus::{Connection, interface};

use std::sync::Arc;
use tokio::sync::Mutex;

/// Handle to the current screenshot session, allowing the D-Bus service
/// to forward `activate()` and `receive_window_data()` calls.
#[derive(Clone)]
pub struct SessionHandle {
    /// Sends a signal to trigger a new screenshot session.
    pub activate_tx: tokio::sync::watch::Sender<bool>,
    /// Sends window data received from KWin scripts.
    pub window_data_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<String>>>>,
}

struct TakeashotService {
    handle: SessionHandle,
}

#[interface(name = "com.takeashot.Service")]
impl TakeashotService {
    async fn activate(&self) {
        tracing::info!("activate() called via D-Bus");
        let _ = self.handle.activate_tx.send(true);
    }

    async fn receive_window_data(&self, json_str: String) {
        tracing::debug!("received window data ({} bytes)", json_str.len());
        let mut guard = self.handle.window_data_tx.lock().await;
        if let Some(tx) = guard.take() {
            let _ = tx.send(json_str);
        } else {
            tracing::warn!("received window data but no session is waiting for it");
        }
    }
}

/// Proxy for calling methods on an existing `com.takeashot.service` instance.
#[zbus::proxy(
    interface = "com.takeashot.Service",
    default_service = "com.takeashot.service",
    default_path = "/com/takeashot/Service"
)]
trait TakeashotServiceProxy {
    fn activate(&self) -> zbus::Result<()>;
}

/// Try to register `com.takeashot.service` on the given connection.
///
/// - On success: the service is registered on `conn`.
/// - On failure: calls `activate()` on the existing instance and returns an error
///   so the caller can exit.
pub async fn register_or_activate(conn: &Connection, handle: SessionHandle) -> Result<()> {
    let service = TakeashotService {
        handle: handle.clone(),
    };

    conn.object_server()
        .at("/com/takeashot/Service", service)
        .await
        .ok(); // Ignore error — object may already be registered on this connection.

    let request_result = conn.request_name("com.takeashot.service").await;

    match request_result {
        Ok(_) => {
            tracing::info!("registered com.takeashot.service on session bus");
            Ok(())
        }
        Err(_) => {
            tracing::info!("com.takeashot.service already owned, activating existing instance");
            match TakeashotServiceProxyProxy::new(conn).await {
                Ok(proxy) => {
                    if let Err(activate_err) = proxy.activate().await {
                        tracing::warn!("failed to call activate() on existing instance: {activate_err}");
                    }
                }
                Err(proxy_err) => {
                    tracing::warn!("failed to create proxy to existing instance: {proxy_err}");
                }
            }
            anyhow::bail!("another instance is already running")
        }
    }
}
