mod app;
mod hotkey;
mod single_instance;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("takeashot starting");

    // Create the App and a handle for D-Bus to call back into.
    let (app, handle) = app::App::new();

    // Try to register our D-Bus service. If another instance is already
    // running, call its activate() and exit.
    let _conn = match single_instance::register_or_activate(handle.clone()).await {
        Ok(conn) => conn,
        Err(e) => {
            tracing::info!("{e:#}");
            std::process::exit(0);
        }
    };

    // Start the Pause-key hotkey listener in a background task.
    tokio::spawn(hotkey::listen(handle.activate_tx.clone()));

    // Run the main loop.
    app.run().await
}
