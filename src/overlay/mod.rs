pub mod renderer;

use anyhow::{Context, Result};
use renderer::{Gpu, SelectionUniform};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers};
use smithay_client_toolkit::seat::pointer::{PointerHandler, ThemedPointer};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface};
use wayland_client::{Connection, Proxy, QueueHandle};

use crate::capture;
use crate::capture::CapturedScreen;
use crate::geom::Rect;
use crate::selection::{CursorShape, SelectionState};

struct OutputOverlay {
    layer: LayerSurface,
    output_name: Option<String>,
    /// Logical position of this output in the global compositor space.
    output_pos: (i32, i32),
    width: u32,
    height: u32,
    scale_factor: i32,
    configured: bool,
    wgpu_surface: Option<wgpu::Surface<'static>>,
    bg_bind_group: Option<wgpu::BindGroup>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    selection_buffer: Option<wgpu::Buffer>,
    selection_bind_group: Option<wgpu::BindGroup>,
    /// Pre-allocated vertex buffer for selection geometry (handles + border).
    selection_vbuf: Option<wgpu::Buffer>,
}

struct OverlayState {
    registry_state: RegistryState,
    compositor: CompositorState,
    output_state: OutputState,
    seat_state: SeatState,
    layer_shell: LayerShell,
    shm_state: Shm,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    themed_pointer: Option<ThemedPointer>,
    overlays: Vec<OutputOverlay>,
    gpu: Gpu,
    captured: Vec<CapturedScreen>,
    exit_requested: bool,
    display_ptr: *mut std::ffi::c_void,
    selection: SelectionState,
    /// Current cursor shape (to avoid redundant set_cursor calls).
    current_cursor: Option<CursorShape>,
    /// Whether the display needs re-rendering (selection changed, etc.)
    dirty: bool,
}

delegate_compositor!(OverlayState);
delegate_output!(OverlayState);
delegate_layer!(OverlayState);
delegate_seat!(OverlayState);
delegate_keyboard!(OverlayState);
delegate_pointer!(OverlayState);
delegate_registry!(OverlayState);
delegate_shm!(OverlayState);

// SAFETY: display_ptr is valid for the lifetime of the Wayland connection.
unsafe impl Send for OverlayState {}

impl OverlayState {
    fn create_overlays(&mut self, qh: &QueueHandle<Self>) {
        let outputs: Vec<_> = self.output_state.outputs().collect();
        if outputs.is_empty() {
            tracing::warn!("no outputs found, creating overlay without output target");
            self.create_layer_surface(qh, None, None, (0, 0));
            return;
        }
        for output in &outputs {
            let info = self.output_state.info(output);
            let name = info.as_ref().and_then(|i| i.name.clone());
            let size = info.as_ref().and_then(|i| i.logical_size).unwrap_or((1920, 1080));
            let pos = info.as_ref().and_then(|i| i.logical_position).unwrap_or((0, 0));
            tracing::info!("creating overlay for output '{name:?}': {size:?} at {pos:?}");
            self.create_layer_surface(qh, Some(output), name, pos);
        }
    }

    fn create_layer_surface(
        &mut self,
        qh: &QueueHandle<Self>,
        output: Option<&wl_output::WlOutput>,
        output_name: Option<String>,
        output_pos: (i32, i32),
    ) {
        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh, surface, Layer::Overlay, Some("takeashot"), output,
        );
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);

        let scale = output
            .and_then(|o| self.output_state.info(o))
            .map(|info| info.scale_factor)
            .unwrap_or(1);
        layer.wl_surface().set_buffer_scale(scale);

        layer.commit();
        self.overlays.push(OutputOverlay {
            layer, output_name, output_pos, width: 0, height: 0, scale_factor: scale,
            configured: false,
            wgpu_surface: None, bg_bind_group: None, surface_config: None,
            selection_buffer: None, selection_bind_group: None,
            selection_vbuf: None,
        });
        tracing::info!("layer surface created and committed (scale={scale})");
    }

    fn find_captured(&self, output_name: &Option<String>) -> Option<&CapturedScreen> {
        if let Some(name) = output_name {
            if let Some(cap) = self.captured.iter().find(|c| &c.name == name) {
                return Some(cap);
            }
        }
        self.captured.first()
    }

    fn global_selection_rect(&self) -> Option<Rect> {
        self.selection.selection.rect().copied()
    }

    /// Update selection uniform buffers + vertex buffers, then render all overlays.
    fn render_all(&mut self) {
        let global_rect = self.global_selection_rect();

        // Pre-compute local selection rects and overlay metadata (avoid borrow conflicts)
        let local_data: Vec<(Option<Rect>, u32, u32)> = self.overlays.iter().map(|o| {
            let local = match &global_rect {
                Some(gr) => {
                    let local = gr.translate(-o.output_pos.0, -o.output_pos.1);
                    let bounds = Rect::new(0, 0, o.width as i32, o.height as i32);
                    local.intersect(&bounds)
                }
                None => None,
            };
            (local, o.width, o.height)
        }).collect();

        for (idx, overlay) in self.overlays.iter_mut().enumerate() {
            if !overlay.configured { continue; }

            // Ensure selection buffers exist
            if overlay.selection_buffer.is_none() {
                overlay.selection_buffer = Some(self.gpu.create_selection_buffer());
                overlay.selection_bind_group = Some(
                    self.gpu.create_selection_bind_group(overlay.selection_buffer.as_ref().unwrap())
                );
            }
            if overlay.selection_vbuf.is_none() {
                overlay.selection_vbuf = Some(self.gpu.create_selection_vertex_buffer());
            }

            // Update selection uniform
            let uniform = match global_rect {
                Some(_) => match local_data[idx].0 {
                    Some(r) => SelectionUniform::from_rect(&r, (local_data[idx].1, local_data[idx].2)),
                    None => SelectionUniform::none(),
                },
                None => SelectionUniform::none(),
            };
            self.gpu.queue.write_buffer(
                overlay.selection_buffer.as_ref().unwrap(), 0,
                bytemuck::bytes_of(&uniform),
            );

            // Update selection vertex buffer
            let verts = match local_data[idx].0 {
                Some(r) => Gpu::build_selection_vertices(&r, (local_data[idx].1, local_data[idx].2)),
                None => Vec::new(),
            };
            let vert_count = verts.len() as u32;
            if !verts.is_empty() {
                self.gpu.queue.write_buffer(
                    overlay.selection_vbuf.as_ref().unwrap(), 0,
                    bytemuck::cast_slice(&verts),
                );
            }

            // Render
            if let (Some(surface), Some(config), Some(bg), Some(sel_bg)) =
                (&overlay.wgpu_surface, &overlay.surface_config, &overlay.bg_bind_group, &overlay.selection_bind_group)
            {
                let sel_verts = if vert_count > 0 {
                    Some((overlay.selection_vbuf.as_ref().unwrap(), vert_count))
                } else {
                    None
                };
                if let Err(e) = self.gpu.render(surface, config, bg, sel_bg, sel_verts) {
                    tracing::warn!("render failed: {e}");
                }
            }
        }

        self.dirty = false;
    }

    /// Update cursor shape based on selection state + pointer position.
    fn update_cursor(&mut self, conn: &Connection, pos: (f64, f64)) {
        let shape = self.selection.cursor_for_position(pos);
        if self.current_cursor == Some(shape) {
            return;
        }
        self.current_cursor = Some(shape);

        if let Some(tp) = &self.themed_pointer {
            let icon = match shape {
                CursorShape::Crosshair => smithay_client_toolkit::seat::pointer::CursorIcon::Crosshair,
                CursorShape::Move => smithay_client_toolkit::seat::pointer::CursorIcon::Move,
                CursorShape::ResizeNWSE => smithay_client_toolkit::seat::pointer::CursorIcon::NwseResize,
                CursorShape::ResizeNESW => smithay_client_toolkit::seat::pointer::CursorIcon::NeswResize,
                CursorShape::ResizeNS => smithay_client_toolkit::seat::pointer::CursorIcon::NsResize,
                CursorShape::ResizeEW => smithay_client_toolkit::seat::pointer::CursorIcon::EwResize,
            };
            if let Err(e) = tp.set_cursor(conn, icon) {
                tracing::debug!("set_cursor failed: {e}");
            }
        }
    }
}

impl CompositorHandler for OverlayState {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, scale: i32) {
        if let Some(overlay) = self.overlays.iter_mut().find(|o| o.layer.wl_surface() == surface) {
            tracing::info!("scale factor changed: {} → {}", overlay.scale_factor, scale);
            overlay.scale_factor = scale;
            surface.set_buffer_scale(scale);
        }
    }
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        if self.dirty {
            self.render_all();
        }
    }
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

        let idx = match self.overlays.iter().position(|o| o.layer.wl_surface() == layer.wl_surface()) {
            Some(i) => i,
            None => return,
        };

        let output_name = self.overlays[idx].output_name.clone();
        let needs_init = self.overlays[idx].wgpu_surface.is_none();
        let scale = self.overlays[idx].scale_factor.max(1);

        let phys_w = w * scale as u32;
        let phys_h = h * scale as u32;

        self.overlays[idx].width = w;
        self.overlays[idx].height = h;
        self.overlays[idx].configured = true;

        if needs_init {
            let wl_surf = layer.wl_surface();
            let surface_ptr = wl_surf.id().as_ptr() as *mut std::ffi::c_void;

            tracing::info!("creating wgpu surface (display_ptr={:p}, surface_ptr={:p})", self.display_ptr, surface_ptr);

            let wgpu_surf = match self.gpu.create_surface_from_wayland(self.display_ptr, surface_ptr) {
                Ok(s) => {
                    tracing::info!("wgpu surface created successfully, logical={w}x{h}, physical={phys_w}x{phys_h}, scale={scale}");
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
                width: phys_w, height: phys_h,
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            wgpu_surf.configure(&self.gpu.device, &config);

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

            let sel_buf = self.gpu.create_selection_buffer();
            let sel_bg = self.gpu.create_selection_bind_group(&sel_buf);
            self.gpu.queue.write_buffer(&sel_buf, 0, bytemuck::bytes_of(&SelectionUniform::none()));
            let sel_vbuf = self.gpu.create_selection_vertex_buffer();

            self.overlays[idx].wgpu_surface = Some(wgpu_surf);
            self.overlays[idx].bg_bind_group = Some(bg_bind_group);
            self.overlays[idx].surface_config = Some(config);
            self.overlays[idx].selection_buffer = Some(sel_buf);
            self.overlays[idx].selection_bind_group = Some(sel_bg);
            self.overlays[idx].selection_vbuf = Some(sel_vbuf);
        } else if self.overlays[idx].wgpu_surface.is_some() && self.overlays[idx].surface_config.is_some() {
            self.overlays[idx].surface_config.as_mut().unwrap().width = phys_w;
            self.overlays[idx].surface_config.as_mut().unwrap().height = phys_h;
            let surf = self.overlays[idx].wgpu_surface.as_ref().unwrap();
            let config = self.overlays[idx].surface_config.as_ref().unwrap();
            surf.configure(&self.gpu.device, config);
        }

        self.dirty = true;
        // Render immediately for configure events (initial setup)
        self.render_all();
    }
}

impl SeatHandler for OverlayState {
    fn seat_state(&mut self) -> &mut SeatState { &mut self.seat_state }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            match self.seat_state.get_keyboard(qh, &seat, None) {
                Ok(kb) => { tracing::info!("keyboard attached"); self.keyboard = Some(kb); }
                Err(e) => tracing::warn!("failed to get keyboard: {e}"),
            }
        }
        if cap == Capability::Pointer && self.themed_pointer.is_none() {
            let cursor_surface = self.overlays.first()
                .map(|o| o.layer.wl_surface().clone())
                .unwrap_or_else(|| self.compositor.create_surface(qh));

            match self.seat_state.get_pointer_with_theme(
                qh, &seat, self.shm_state.wl_shm(), cursor_surface,
                smithay_client_toolkit::seat::pointer::ThemeSpec::default(),
            ) {
                Ok(tp) => {
                    tracing::info!("pointer attached with theme");
                    self.themed_pointer = Some(tp);
                }
                Err(e) => tracing::warn!("failed to get themed pointer: {e}"),
            }
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Keyboard { self.keyboard = None; }
        if cap == Capability::Pointer { self.themed_pointer = None; }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for OverlayState {
    fn pointer_frame(
        &mut self,
        conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[smithay_client_toolkit::seat::pointer::PointerEvent],
    ) {
        use smithay_client_toolkit::seat::pointer::PointerEventKind;

        for event in events {
            let (x, y) = event.position;

            let global_pos = self.overlays.iter()
                .find(|o| o.layer.wl_surface() == &event.surface)
                .map(|o| (x + o.output_pos.0 as f64, y + o.output_pos.1 as f64))
                .unwrap_or((x, y));

            match &event.kind {
                PointerEventKind::Press { button, .. } => {
                    self.selection.on_pointer_press(global_pos, *button);
                }
                PointerEventKind::Release { button, .. } => {
                    self.selection.on_pointer_release(global_pos, *button);
                }
                PointerEventKind::Motion { .. } => {
                    self.selection.on_pointer_motion(global_pos);
                }
                PointerEventKind::Enter { .. } | PointerEventKind::Leave { .. } => {}
                PointerEventKind::Axis { .. } => {}
            }

            self.update_cursor(conn, global_pos);
        }

        // Just mark dirty — the main loop or frame callback will render
        self.dirty = true;
    }
}

impl KeyboardHandler for OverlayState {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {}
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {}
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        if event.keysym == Keysym::Escape {
            if self.selection.on_escape() {
                tracing::info!("Esc pressed, exiting overlay");
                self.exit_requested = true;
            } else {
                tracing::info!("Esc pressed, selection cleared");
                self.dirty = true;
            }
        } else if event.keysym == Keysym::Return {
            match self.selection.on_enter() {
                crate::selection::ConfirmAction::Confirmed { rect } => {
                    tracing::info!("Enter pressed, selection confirmed: {rect:?}");
                }
                crate::selection::ConfirmAction::NoSelection => {}
            }
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

fn get_display_ptr(conn: &Connection) -> *mut std::ffi::c_void {
    conn.backend().display_ptr() as *mut std::ffi::c_void
}

/// Run the overlay event loop. Blocks until Esc is pressed or compositor closes.
pub fn run(dbus_conn: zbus::Connection) -> Result<()> {
    run_inner(dbus_conn, None)
}

/// Run the overlay event loop with an auto-exit timeout.
pub fn run_with_timeout(
    dbus_conn: zbus::Connection,
    timeout: std::time::Duration,
) -> Result<()> {
    run_inner(dbus_conn, Some(timeout))
}

fn run_inner(
    dbus_conn: zbus::Connection,
    timeout: Option<std::time::Duration>,
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
        keyboard: None, themed_pointer: None, overlays: Vec::new(), gpu, captured: Vec::new(),
        exit_requested: false, display_ptr,
        selection: SelectionState::new(),
        current_cursor: None,
        dirty: false,
    };

    event_queue.flush()?;
    let rounds = conn.roundtrip()?;
    tracing::info!("initial roundtrip for output info ({rounds} rounds)");

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

    state.captured = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(
            capture::capture_all(&dbus_conn, &output_names)
        )
    })?;

    state.create_overlays(&qh);

    event_queue.flush()?;
    let rounds = conn.roundtrip()?;
    tracing::info!("second roundtrip for configure events ({rounds} rounds)");

    tracing::info!("overlay event loop starting");

    let deadline = timeout.map(|dur| {
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
                unsafe { libc::poll(&mut pfd, 1, ms); }
            }
            if let Some(guard) = conn.prepare_read() {
                let _ = guard.read();
            }
            event_queue.dispatch_pending(&mut state)?;
        } else {
            event_queue.blocking_dispatch(&mut state)?;
        }

        // Render after processing events, if anything changed
        if state.dirty {
            state.render_all();
        }
    }

    tracing::info!("overlay exiting");
    Ok(())
}
