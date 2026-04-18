mod annotation;
mod app;
mod capture;
mod clipboard;
mod compose;
mod geom;
mod hotkey;
mod kwin;
mod overlay;
mod selection;
mod single_instance;
mod snap;
mod ui;

use anyhow::Result;

/// Command-line arguments.
#[derive(Debug, clap::Parser)]
#[command(name = "takeashot", about = "KDE Wayland screenshot tool")]
struct Args {
    /// Start a screenshot immediately instead of waiting for hotkey.
    #[arg(short, long)]
    now: bool,

    /// Smoke test: skip single-instance check, show overlay, auto-exit after 3 seconds.
    #[arg(long)]
    smoke: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Args = clap::Parser::parse();

    // Initialize logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("takeashot starting");

    // Connect to session bus early — shared for single-instance and KWin calls.
    let dbus_conn = zbus::Connection::session()
        .await
        .map_err(|e| anyhow::anyhow!("failed to connect to session bus: {e}"))?;

    if args.smoke {
        tracing::info!("smoke test mode");

        // Register a smoke-mode D-Bus service with a random name to avoid
        // conflicts with any running main instance.
        let smoke_service_name = format!("com.takeashot.smoke.s{}", std::process::id());
        let window_data_tx: std::sync::Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(None));

        // Register the object server so KWin's callDBus can reach us
        single_instance::register_smoke_service(&dbus_conn, &window_data_tx, &smoke_service_name).await;

        // Fetch window list (best-effort; snap disabled on failure)
        let windows = kwin::windows::fetch_window_list(
            &dbus_conn,
            &window_data_tx,
            &smoke_service_name,
        ).await.unwrap_or_else(|e| {
            tracing::warn!("smoke: failed to fetch window list, snap disabled: {e:#}");
            Vec::new()
        });

        overlay::run_with_timeout(dbus_conn, windows, std::time::Duration::from_secs(3))?;
        tracing::info!("smoke test passed");
        return Ok(());
    }

    // Create the App and a handle for D-Bus to call back into.
    let (app, handle) = app::App::new(dbus_conn.clone());

    // Try to register our D-Bus service. If another instance is already
    // running, call its activate() and exit.
    if let Err(e) = single_instance::register_or_activate(&dbus_conn, handle.clone()).await {
        tracing::info!("{e:#}");
        std::process::exit(0);
    }

    // Start the Pause-key hotkey listener in a background task.
    tokio::spawn(hotkey::listen(handle.activate_tx.clone()));

    // If --now flag is set, trigger a capture immediately.
    if args.now {
        tracing::info!("--now flag: starting capture immediately");
        handle.activate_tx.send(true).ok();
    }

    // Run the main loop.
    app.run().await
}