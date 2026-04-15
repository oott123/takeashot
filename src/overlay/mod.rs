pub mod renderer;

use anyhow::{Context, Result};
use renderer::Gpu;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_registry,
    delegate_seat, delegate_shm,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_seat, wl_surface};
use wayland_client::{Connection, Proxy, QueueHandle};

use crate::capture;
use crate::capture::CapturedScreen;

struct OutputOverlay {
    layer: LayerSurface,
    output_name: Option<String>,
    width: u32,
    height: u32,
    configured: bool,
    wgpu_surface: Option<wgpu::Surface<'static>>,
    bg_bind_group: Option<wgpu::BindGroup>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
}

struct OverlayState {
    registry_state: RegistryState,
    compositor: CompositorState,
    output_state: OutputState,
    seat_state: SeatState,
    layer_shell: LayerShell,
    shm_state: Shm,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    overlays: Vec<OutputOverlay>,
    gpu: Gpu,
    captured: Vec<CapturedScreen>,
    exit_requested: bool,
    display_ptr: *mut std::ffi::c_void,
}

delegate_compositor!(OverlayState);
delegate_output!(OverlayState);
delegate_layer!(OverlayState);
delegate_seat!(OverlayState);
delegate_keyboard!(OverlayState);
delegate_registry!(OverlayState);
delegate_shm!(OverlayState);

// SAFETY: display_ptr is valid for the lifetime of the Wayland connection.
unsafe impl Send for OverlayState {}

impl OverlayState {
    fn create_overlays(&mut self, qh: &QueueHandle<Self>) {
        let outputs: Vec<_> = self.output_state.outputs().collect();
        if outputs.is_empty() {
            tracing::warn!("no outputs found, creating overlay without output target");
            self.create_layer_surface(qh, None, None);
            return;
        }
        for output in &outputs {
            let info = self.output_state.info(output);
            let name = info.as_ref().and_then(|i| i.name.clone());
            let size = info.and_then(|i| i.logical_size).unwrap_or((1920, 1080));
            tracing::info!("creating overlay for output '{name:?}': {size:?}");
            self.create_layer_surface(qh, Some(output), name);
        }
    }

    fn create_layer_surface(
        &mut self,
        qh: &QueueHandle<Self>,
        output: Option<&wl_output::WlOutput>,
        output_name: Option<String>,
    ) {
        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh, surface, Layer::Overlay, Some("takeashot"), output,
        );
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer.commit();
        self.overlays.push(OutputOverlay {
            layer, output_name, width: 0, height: 0, configured: false,
            wgpu_surface: None, bg_bind_group: None, surface_config: None,
        });
        tracing::info!("layer surface created and committed");
    }

    fn find_captured(&self, output_name: &Option<String>) -> Option<&CapturedScreen> {
        // Try to match by output name first
        if let Some(name) = output_name {
            if let Some(cap) = self.captured.iter().find(|c| &c.name == name) {
                return Some(cap);
            }
        }
        // Fallback: use first captured screen
        self.captured.first()
    }

    fn render_all(&mut self) {
        for overlay in &mut self.overlays {
            if !overlay.configured { continue; }
            if let (Some(surface), Some(config), Some(bg)) = (&overlay.wgpu_surface, &overlay.surface_config, &overlay.bg_bind_group) {
                if let Err(e) = self.gpu.render(surface, config, bg) {
                    tracing::warn!("render failed: {e}");
                }
            }
        }
    }
}

impl CompositorHandler for OverlayState {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) { self.render_all(); }
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for OverlayState {
    fn output_state(&mut self) -> &mut OutputState { &mut self.output_state }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for OverlayState {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        tracing::info!("layer surface closed");
        self.exit_requested = true;
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface, configure: LayerSurfaceConfigure, _serial: u32) {
        let (w, h) = configure.new_size;
        tracing::info!("layer surface configure: {w}x{h}");
        if w == 0 || h == 0 { return; }

        // Find the index of the matching overlay
        let idx = match self.overlays.iter().position(|o| o.layer.wl_surface() == layer.wl_surface()) {
            Some(i) => i,
            None => return,
        };

        let output_name = self.overlays[idx].output_name.clone();
        let needs_init = self.overlays[idx].wgpu_surface.is_none();

        self.overlays[idx].width = w;
        self.overlays[idx].height = h;
        self.overlays[idx].configured = true;

        if needs_init {
            let wl_surf = layer.wl_surface();
            let surface_ptr = wl_surf.id().as_ptr() as *mut std::ffi::c_void;

            tracing::info!("creating wgpu surface (display_ptr={:p}, surface_ptr={:p})", self.display_ptr, surface_ptr);

            let wgpu_surf = match self.gpu.create_surface_from_wayland(self.display_ptr, surface_ptr) {
                Ok(s) => {
                    tracing::info!("wgpu surface created successfully, size={w}x{h}");
                    s
                }
                Err(e) => {
                    tracing::error!("failed to create wgpu surface: {e:#}");
                    return;
                }
            };

            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                width: w, height: h,
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            wgpu_surf.configure(&self.gpu.device, &config);

            // Find the captured screen matching this output's name
            let cap = self.find_captured(&output_name);
            let bg_bind_group = match cap {
                Some(cap) => {
                    tracing::info!("uploading texture for output '{output_name:?}' ({}x{})",
                        cap.width, cap.height);
                    self.gpu.upload_bgra_texture(cap.width, cap.height, cap.stride, &cap.bgra)
                }
                None => {
                    tracing::warn!("no captured screen data for output '{output_name:?}'");
                    return;
                }
            };

            self.overlays[idx].wgpu_surface = Some(wgpu_surf);
            self.overlays[idx].bg_bind_group = Some(bg_bind_group);
            self.overlays[idx].surface_config = Some(config);
        } else if self.overlays[idx].wgpu_surface.is_some() && self.overlays[idx].surface_config.is_some() {
            self.overlays[idx].surface_config.as_mut().unwrap().width = w;
            self.overlays[idx].surface_config.as_mut().unwrap().height = h;
            let surf = self.overlays[idx].wgpu_surface.as_ref().unwrap();
            let config = self.overlays[idx].surface_config.as_ref().unwrap();
            surf.configure(&self.gpu.device, config);
        }

        if let (Some(surface), Some(config), Some(bg)) = (&self.overlays[idx].wgpu_surface, &self.overlays[idx].surface_config, &self.overlays[idx].bg_bind_group) {
            if let Err(e) = self.gpu.render(surface, config, bg) {
                tracing::warn!("initial render failed: {e}");
            } else {
                tracing::info!("initial render succeeded");
            }
        }
    }
}

impl SeatHandler for OverlayState {
    fn seat_state(&mut self) -> &mut SeatState { &mut self.seat_state }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            match self.seat_state.get_keyboard(qh, &seat, None) {
                Ok(kb) => { tracing::info!("keyboard attached"); self.keyboard = Some(kb); }
                Err(e) => tracing::warn!("failed to get keyboard: {e}"),
            }
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Keyboard { self.keyboard = None; }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for OverlayState {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {}
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {}
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        if event.keysym == Keysym::Escape {
            tracing::info!("Esc pressed, exiting overlay");
            self.exit_requested = true;
        }
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: Modifiers, _: RawModifiers, _: u32) {}
    fn repeat_key(&mut self, c: &Connection, q: &QueueHandle<Self>, k: &wl_keyboard::WlKeyboard, s: u32, e: KeyEvent) { self.press_key(c, q, k, s, e); }
}

impl ShmHandler for OverlayState {
    fn shm_state(&mut self) -> &mut Shm { &mut self.shm_state }
}

impl ProvidesRegistryState for OverlayState {
    fn registry(&mut self) -> &mut RegistryState { &mut self.registry_state }
    fn runtime_add_global(&mut self, _: &Connection, _: &QueueHandle<Self>, _: u32, _: &str, _: u32) {}
    fn runtime_remove_global(&mut self, _: &Connection, _: &QueueHandle<Self>, _: u32, _: &str) {}
}

/// Extract the raw wl_display pointer from a Wayland Connection.
fn get_display_ptr(conn: &Connection) -> *mut std::ffi::c_void {
    conn.backend().display_ptr() as *mut std::ffi::c_void
}

/// Run the overlay event loop. Blocks until Esc is pressed or compositor closes.
///
/// The D-Bus connection is used for per-screen KWin captures after outputs are enumerated.
pub fn run(dbus_conn: zbus::Connection) -> Result<()> {
    run_inner(dbus_conn, None)
}

/// Run the overlay event loop with an auto-exit timeout.
/// After `timeout` elapses, the overlay closes automatically.
pub fn run_with_timeout(
    dbus_conn: zbus::Connection,
    pre_captured: Vec<CapturedScreen>,
    timeout: std::time::Duration,
) -> Result<()> {
    run_inner(dbus_conn, Some((pre_captured, timeout)))
}

fn run_inner(
    dbus_conn: zbus::Connection,
    smoke: Option<(Vec<CapturedScreen>, std::time::Duration)>,
) -> Result<()> {
    let conn = Connection::connect_to_env().context("failed to connect to Wayland display")?;
    let display_ptr = get_display_ptr(&conn);

    let (globals, mut event_queue) = registry_queue_init(&conn).context("failed to init Wayland registry")?;
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).context("failed to bind compositor")?;
    let layer_shell = LayerShell::bind(&globals, &qh).context("failed to bind layer shell")?;
    let shm = Shm::bind(&globals, &qh).context("failed to bind SHM")?;
    let output_state = OutputState::new(&globals, &qh);
    let seat_state = SeatState::new(&globals, &qh);
    let gpu = pollster::block_on(Gpu::new())?;

    let mut state = OverlayState {
        registry_state: RegistryState::new(&globals),
        compositor, output_state, seat_state, layer_shell, shm_state: shm,
        keyboard: None, overlays: Vec::new(), gpu, captured: Vec::new(), exit_requested: false,
        display_ptr,
    };

    // Roundtrip to receive output geometry/mode info before creating overlays.
    event_queue.flush()?;
    let rounds = conn.roundtrip()?;
    tracing::info!("initial roundtrip for output info ({rounds} rounds)");

    // The xdg_output name events may arrive after the initial wl_output done.
    // Use blocking_dispatch which also processes SCTK callbacks.
    let mut output_names: Vec<String> = Vec::new();
    for attempt in 0..5 {
        output_names = state.output_state.outputs()
            .filter_map(|o| state.output_state.info(&o))
            .filter_map(|info| info.name.clone())
            .collect();
        if !output_names.is_empty() {
            break;
        }
        tracing::info!("output names not yet available, dispatching ({attempt})...");
        event_queue.blocking_dispatch(&mut state)?;
    }
    tracing::info!("detected outputs: {output_names:?}");

    // Do per-screen capture via KWin D-Bus (unless smoke mode provides pre-captured data).
    let smoke_timeout = smoke.as_ref().map(|(_, dur)| *dur);
    if let Some((pre_captured, _)) = smoke {
        state.captured = pre_captured;
    } else {
        state.captured = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(
                capture::capture_all(&dbus_conn, &output_names)
            )
        })?;
    }

    state.create_overlays(&qh);

    // Second roundtrip: compositor processes layer surface creation and
    // sends configure events with actual surface dimensions.
    event_queue.flush()?;
    let rounds = conn.roundtrip()?;
    tracing::info!("second roundtrip for configure events ({rounds} rounds)");

    tracing::info!("overlay event loop starting");

    let deadline = smoke_timeout.map(|dur| {
        let instant = std::time::Instant::now() + dur;
        tracing::info!("smoke mode: will auto-exit in {:?}", dur);
        instant
    });

    while !state.exit_requested {
        if let Some(dl) = deadline {
            if std::time::Instant::now() >= dl {
                tracing::info!("smoke timeout reached, exiting");
                break;
            }
            // Non-blocking dispatch + poll with remaining timeout
            event_queue.dispatch_pending(&mut state)?;
            event_queue.flush()?;

            let remaining = dl.saturating_duration_since(std::time::Instant::now());
            if !remaining.is_zero() {
                use std::os::fd::{AsFd, AsRawFd};
                let wayland_fd = conn.as_fd().as_raw_fd();
                let mut pfd = libc::pollfd {
                    fd: wayland_fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                let ms = remaining.as_millis() as i32;
                // SAFETY: poll() on a single fd with timeout
                unsafe { libc::poll(&mut pfd, 1, ms); }
            }
            // Read and dispatch what arrived
            if let Some(guard) = conn.prepare_read() {
                let _ = guard.read();
            }
            event_queue.dispatch_pending(&mut state)?;
        } else {
            event_queue.blocking_dispatch(&mut state)?;
        }
    }

    tracing::info!("overlay exiting");
    Ok(())
}
