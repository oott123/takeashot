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

use crate::annotation::AnnotationState;
use crate::capture;
use crate::capture::CapturedScreen;
use crate::geom::Rect;
use crate::kwin::windows::WindowInfo;
use crate::selection::{CursorShape, SelectionState};
use crate::ui::toolbar::Tool;
use crate::ui::EguiState;

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
    /// Source texture view for this output's screenshot. Retained so that the
    /// blurred texture can be regenerated lazily when the mosaic tool runs.
    bg_view: Option<wgpu::TextureView>,
    /// Source texture size (matches the captured screenshot, not the surface).
    bg_size: Option<(u32, u32)>,
    /// Blurred texture bind group for mosaic rendering. Lazily generated the
    /// first time a mosaic quad needs to be drawn, and invalidated when the
    /// blur-pass count changes.
    blurred_bind_group: Option<wgpu::BindGroup>,
    /// Pass count used to build the current `blurred_bind_group`.
    blur_passes_used: Option<u32>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    selection_buffer: Option<wgpu::Buffer>,
    selection_bind_group: Option<wgpu::BindGroup>,
    /// Pre-allocated vertex buffer for selection geometry (handles + border).
    selection_vbuf: Option<wgpu::Buffer>,
    /// Pre-allocated vertex buffer for annotation geometry.
    annotation_vbuf: Option<wgpu::Buffer>,
    /// Pre-allocated vertex buffer for mosaic quad geometry.
    mosaic_vbuf: Option<wgpu::Buffer>,
}

/// Which subsystem owns the current pointer drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PointerOwner {
    /// No drag in progress — next Press decides ownership.
    None,
    /// egui (toolbar) owns the drag.
    Egui,
    /// Selection/annotation system owns the drag.
    Overlay,
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
    blur: crate::blur::DualBlur,
    /// Current cursor shape (to avoid redundant set_cursor calls).
    current_cursor: Option<CursorShape>,
    /// Whether the display needs re-rendering (selection changed, etc.)
    dirty: bool,
    /// Annotation state (shapes, drawing, editing).
    annotations: AnnotationState,
    /// Currently active tool.
    tool: Tool,
    /// Blur-pass count used by the Mosaic tool. Bumping this invalidates each
    /// output's cached blurred texture so it re-blurs on the next render.
    blur_passes: u32,
    /// Egui state for toolbar rendering.
    egui: EguiState,
    /// Which subsystem owns the current pointer drag.
    pointer_owner: PointerOwner,
    /// Window list from KWin (for snap matching). Empty if fetch failed.
    windows: Vec<WindowInfo>,
    /// Last left-click time and position for double-click detection.
    last_click: Option<(u32, (f64, f64))>,
    /// Current keyboard modifier state (Alt/Shift force "new shape" on press).
    modifiers: Modifiers,
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
            wgpu_surface: None, bg_bind_group: None, bg_view: None, bg_size: None,
            blurred_bind_group: None, blur_passes_used: None, surface_config: None,
            selection_buffer: None, selection_bind_group: None,
            selection_vbuf: None, annotation_vbuf: None, mosaic_vbuf: None,
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

    /// Find the output that should display the toolbar for the given global selection rect.
    /// Uses the selection center to determine ownership — this always lands inside the
    /// correct output even when the selection edge is flush with the screen boundary.
    fn find_toolbar_output_idx(&self, sel: Rect) -> Option<usize> {
        let cx = sel.x + sel.w / 2;
        let cy = sel.y + sel.h / 2;
        self.overlays.iter().position(|o| {
            cx >= o.output_pos.0 && cx < o.output_pos.0 + o.width as i32 &&
            cy >= o.output_pos.1 && cy < o.output_pos.1 + o.height as i32
        })
    }

    /// Compute the toolbar bounding rect in global logical coordinates.
    /// Returns None when the selection is not Confirmed — the toolbar
    /// must not appear during snap preview or drag-creation.
    fn compute_toolbar_rect(&self) -> Option<Rect> {
        if !self.selection.selection.is_confirmed() {
            return None;
        }
        let global_rect = self.global_selection_rect()?;
        let overlay = self.overlays.get(self.find_toolbar_output_idx(global_rect)?)?;
        crate::ui::toolbar::toolbar_rect(
            Some(global_rect),
            overlay.output_pos,
            (overlay.width, overlay.height),
        )
    }

    /// Update selection uniform buffers + vertex buffers, then render all overlays.
    fn render_all(&mut self) {
        // Determine which output should display the toolbar.
        let global_rect = self.global_selection_rect();
        // Toolbar is only shown for confirmed selections — not during snap preview or drag-creation.
        let confirmed_rect = if self.selection.selection.is_confirmed() { global_rect } else { None };
        let toolbar_output_idx = confirmed_rect.and_then(|sel| {
            self.find_toolbar_output_idx(sel)
        });

        // Get the toolbar output's parameters for egui
        let (tb_output_pos, tb_output_size, tb_scale) = toolbar_output_idx
            .and_then(|idx| self.overlays.get(idx))
            .map(|o| (o.output_pos, (o.width, o.height), o.scale_factor))
            .unwrap_or_else(|| {
                // Fallback: use first configured output
                self.overlays.iter()
                    .find(|o| o.configured)
                    .map(|o| (o.output_pos, (o.width, o.height), o.scale_factor))
                    .unwrap_or(((0, 0), (0, 0), 1))
            });

        // Initialize egui renderer if needed
        self.egui.init_renderer(&self.gpu.device, wgpu::TextureFormat::Bgra8UnormSrgb);
        self.egui.set_pixels_per_point(tb_scale.max(1) as f32);

        let (tool_change, blur_pass_change) = self.egui.run_ui(
            &self.gpu.device,
            &self.gpu.queue,
            self.tool,
            self.blur_passes,
            confirmed_rect,
            tb_output_pos,
            tb_output_size,
        );
        if let Some(new_tool) = tool_change {
            if new_tool != self.tool {
                tracing::info!("tool changed: {:?} → {:?}", self.tool, new_tool);
                // Every tool switch drops the current annotation selection:
                // selection semantics are scoped to "annotations of the current
                // tool's shape", so carrying it across is meaningless.
                self.annotations.deselect_all();
                self.tool = new_tool;
            }
        }
        if let Some(new_passes) = blur_pass_change {
            if new_passes != self.blur_passes {
                tracing::info!("blur passes changed: {} → {}", self.blur_passes, new_passes);
                self.blur_passes = new_passes;
                // Invalidate cached blurs so they rebuild with the new pass count.
                for o in self.overlays.iter_mut() {
                    o.blurred_bind_group = None;
                    o.blur_passes_used = None;
                }
            }
        }

        // Pre-compute local selection rects and overlay metadata (avoid borrow conflicts)
        let local_data: Vec<(Option<Rect>, u32, u32, i32, (i32, i32))> = self.overlays.iter().map(|o| {
            let local = match &global_rect {
                Some(gr) => {
                    let local = gr.translate(-o.output_pos.0, -o.output_pos.1);
                    let bounds = Rect::new(0, 0, o.width as i32, o.height as i32);
                    local.intersect(&bounds)
                }
                None => None,
            };
            (local, o.width, o.height, o.scale_factor, o.output_pos)
        }).collect();

        // Compute annotation vertex data per output
        let annotations = &self.annotations;
        let drawing_shape = annotations.drawing_shape();
        let drawing_transform = annotations.drawing_transform();
        let edit_handles = annotations.edit_handles();
        let selected_idx = annotations.selected_index();

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
            if overlay.annotation_vbuf.is_none() {
                overlay.annotation_vbuf = Some(self.gpu.create_annotation_vertex_buffer());
            }
            if overlay.mosaic_vbuf.is_none() {
                overlay.mosaic_vbuf = Some(self.gpu.create_mosaic_vertex_buffer());
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
            let include_handles = self.selection.selection.shows_handles();
            let verts = match local_data[idx].0 {
                Some(r) => Gpu::build_selection_vertices(&r, (local_data[idx].1, local_data[idx].2), include_handles),
                None => Vec::new(),
            };
            let vert_count = verts.len() as u32;
            if !verts.is_empty() {
                self.gpu.queue.write_buffer(
                    overlay.selection_vbuf.as_ref().unwrap(), 0,
                    bytemuck::cast_slice(&verts),
                );
            }

            // Tessellate annotations for this output
            let scale = local_data[idx].3;
            let output_rect = Rect::new(
                local_data[idx].4 .0,
                local_data[idx].4 .1,
                local_data[idx].1 as i32,
                local_data[idx].2 as i32,
            );
            let phys_w = local_data[idx].1 * scale.max(1) as u32;
            let phys_h = local_data[idx].2 * scale.max(1) as u32;

            let ann_verts = crate::annotation::render::tessellate_annotations(
                annotations.annotations(),
                drawing_shape,
                drawing_transform,
                None, // drawing_color
                if selected_idx.is_some() { &edit_handles } else { &[] },
                selected_idx.and_then(|idx| {
                    annotations.annotations().get(idx)
                        .map(|ann| crate::annotation::AnnotationState::oriented_bounds(ann))
                }),
                output_rect,
                scale,
                (phys_w, phys_h),
            );
            let ann_vert_count = ann_verts.len() as u32;
            let max_ann_verts = renderer::Gpu::MAX_ANNOTATION_VERTICES as u32;
            if ann_vert_count > max_ann_verts {
                tracing::warn!("annotation vertex count ({ann_vert_count}) exceeds buffer capacity ({max_ann_verts}), truncating");
            }
            let ann_vert_count = ann_vert_count.min(max_ann_verts);
            if !ann_verts.is_empty() {
                let bytes_to_write = ann_vert_count as usize * std::mem::size_of::<renderer::ColoredVertex>();
                self.gpu.queue.write_buffer(
                    overlay.annotation_vbuf.as_ref().unwrap(), 0,
                    &bytemuck::cast_slice(&ann_verts)[..bytes_to_write],
                );
            }

            // Tessellate mosaic quads for this output
            let mos_verts = crate::annotation::render::tessellate_mosaic_quads(
                annotations.annotations(),
                drawing_shape,
                drawing_transform,
                output_rect,
                scale,
                (phys_w, phys_h),
            );
            let mos_vert_count = mos_verts.len() as u32;
            let max_mos_verts = renderer::Gpu::MAX_MOSAIC_VERTICES as u32;
            if mos_vert_count > max_mos_verts {
                tracing::warn!("mosaic vertex count ({mos_vert_count}) exceeds buffer capacity ({max_mos_verts}), truncating");
            }
            let mos_vert_count = mos_vert_count.min(max_mos_verts);
            if !mos_verts.is_empty() {
                let bytes_to_write = mos_vert_count as usize * std::mem::size_of::<renderer::TexturedVertex>();
                self.gpu.queue.write_buffer(
                    overlay.mosaic_vbuf.as_ref().unwrap(), 0,
                    &bytemuck::cast_slice(&mos_verts)[..bytes_to_write],
                );
            }

            // Lazily build (or rebuild) this output's blurred texture when a
            // mosaic quad actually wants to render. Rebuild also fires after
            // the user moves the blur-pass slider, which cleared the cache.
            if mos_vert_count > 0 {
                let needs_blur = overlay.blurred_bind_group.is_none()
                    || overlay.blur_passes_used != Some(self.blur_passes);
                if needs_blur {
                    if let (Some(src_view), Some((bw, bh))) =
                        (overlay.bg_view.as_ref(), overlay.bg_size)
                    {
                        let bg = self.blur.blur(src_view, bw, bh, self.blur_passes);
                        overlay.blurred_bind_group = Some(bg);
                        overlay.blur_passes_used = Some(self.blur_passes);
                    }
                }
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
                let ann_verts_opt = if ann_vert_count > 0 {
                    Some((overlay.annotation_vbuf.as_ref().unwrap(), ann_vert_count))
                } else {
                    None
                };
                let mosaic_opt = match (mos_vert_count, overlay.blurred_bind_group.as_ref()) {
                    (0, _) | (_, None) => None,
                    (_, Some(blurred_bg)) => {
                        Some((blurred_bg, overlay.mosaic_vbuf.as_ref().unwrap(), mos_vert_count))
                    }
                };

                // Acquire surface texture once
                let st = match self.gpu.acquire_surface_texture(surface, config) {
                    Ok(Some(st)) => st,
                    Ok(None) => continue,
                    Err(e) => { tracing::warn!("acquire surface failed: {e}"); continue; }
                };
                let view = st.texture.create_view(&wgpu::TextureViewDescriptor::default());

                // Render main passes (screenshot + mosaic + annotations + handles)
                self.gpu.render_into(&view, bg, sel_bg, sel_verts, ann_verts_opt, mosaic_opt);

                // Render egui toolbar on this output only if it's the toolbar output
                if toolbar_output_idx == Some(idx) {
                    self.egui.paint(&self.gpu.device, &self.gpu.queue, &view, (config.width, config.height));
                }

                st.present();
            }
        }

        self.dirty = false;

        // If egui has active animations (e.g. toolbar fade-in), keep requesting
        // frames so the compositor's frame callback triggers another render.
        if self.egui.ctx.has_requested_repaint() {
            self.dirty = true;
        }
    }

    /// Update cursor shape based on tool + selection state + pointer position.
    fn update_cursor(&mut self, conn: &Connection, pos: (f64, f64)) {
        let shape = match self.tool {
            Tool::Move => self.selection.cursor_for_position(pos),
            Tool::Pen | Tool::Line | Tool::Rect | Tool::Ellipse | Tool::Mosaic => {
                // Only consult the annotation system once the selection is
                // confirmed — before that, the drawing tool isn't usable and
                // the selection state machine should drive the cursor.
                if self.selection.selection.is_confirmed() {
                    let sel_rect = self.selection.selection.rect();
                    self.annotations
                        .cursor_for_position(pos, self.tool, sel_rect)
                        .unwrap_or_else(|| self.selection.cursor_for_position(pos))
                } else {
                    self.selection.cursor_for_position(pos)
                }
            }
        };
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

    /// Compose the final image from the current selection + annotations,
    /// then exit the overlay. Encoding and clipboard write happen in a
    /// background thread so the overlay closes immediately.
    fn confirm_and_exit(&mut self) {
        match self.selection.on_enter() {
            crate::selection::ConfirmAction::Confirmed { rect } => {
                tracing::info!("confirming selection: {rect:?}");
                let output_infos: Vec<crate::compose::OutputInfo> = self.overlays.iter()
                    .filter(|o| o.configured && o.bg_bind_group.is_some())
                    .map(|o| crate::compose::OutputInfo {
                        output_name: o.output_name.clone(),
                        output_pos: o.output_pos,
                        width: o.width,
                        height: o.height,
                        scale_factor: o.scale_factor,
                        bg_bind_group: o.bg_bind_group.clone().unwrap(),
                    })
                    .collect();

                match crate::compose::compose_selection(
                    &self.gpu, &output_infos, &self.captured, &self.annotations, rect, self.blur_passes,
                ) {
                    Ok(img) => {
                        std::thread::spawn(move || {
                            if let Err(e) = crate::clipboard::copy_to_clipboard(img) {
                                tracing::error!("clipboard copy failed: {e:#}");
                            }
                        });
                    }
                    Err(e) => tracing::error!("compose failed: {e:#}"),
                }
                self.exit_requested = true;
            }
            crate::selection::ConfirmAction::NoSelection => {}
        }
    }

    /// Tool-aware pointer press handler.
    fn handle_pointer_press(&mut self, pos: (f64, f64), button: u32, time: u32) {
        // In None/Pending/Creating mode, the only interaction is confirm-snap or drag-create.
        // Tool routing is irrelevant — bypass it entirely.
        if !self.selection.selection.is_confirmed() {
            if self.selection.on_pointer_press(pos, button) {
                self.exit_requested = true;
            }
            return;
        }

        // Right-click in confirmed mode, new rules (drawing tools only):
        //   - active drag/drawing → ignore, keep the drag clean
        //   - annotation selected → deselect, stay in tool
        //   - no selection       → snap back to Move tool
        //   - Move tool          → fall through to selection's cancel path
        if button == 0x111 {
            if self.annotations.drawing_shape().is_some() || self.annotations.has_edit_drag() {
                return;
            }
            if self.annotations.has_selection() {
                self.annotations.deselect_all();
                self.dirty = true;
                return;
            }
            if self.tool != Tool::Move {
                self.tool = Tool::Move;
                self.dirty = true;
                return;
            }
            // Move tool with no annotation selection: let SelectionState handle
            // right-click (clear selection or exit).
        }

        // Double-click detection: left click inside confirmed selection with Move tool
        if button == 0x110 && self.tool == Tool::Move && self.selection.selection.is_confirmed() {
            let inside = self.selection.selection.rect().map_or(false, |r| {
                let p = crate::geom::Point::new(pos.0 as i32, pos.1 as i32);
                r.contains(p)
            });
            if inside {
                if let Some((prev_time, prev_pos)) = self.last_click {
                    let dt = time.wrapping_sub(prev_time);
                    let dist = ((pos.0 - prev_pos.0).abs() + (pos.1 - prev_pos.1).abs()) < 5.0;
                    if dt < 300 && dist {
                        self.last_click = None;
                        self.confirm_and_exit();
                        return;
                    }
                }
            }
        }
        // Track this click for double-click detection (left button only)
        if button == 0x110 {
            self.last_click = Some((time, pos));
        }

        let prev_had_selection = self.global_selection_rect().is_some();

        match self.tool {
            Tool::Move => {
                if self.selection.on_pointer_press(pos, button) {
                    self.exit_requested = true;
                }
            }
            Tool::Pen | Tool::Line | Tool::Rect | Tool::Ellipse | Tool::Mosaic => {
                // Check if pointer is inside confirmed selection
                let inside = self.selection.selection.rect().map_or(false, |r| {
                    let p = crate::geom::Point::new(pos.0 as i32, pos.1 as i32);
                    r.contains(p)
                });
                if inside && self.selection.selection.is_confirmed() {
                    let sel_rect = self.selection.selection.rect().copied();
                    let force_new = self.modifiers.alt || self.modifiers.shift;
                    self.annotations.on_pointer_press(pos, button, self.tool, force_new, sel_rect);
                } else {
                    // Outside selection: route to selection (create/extend)
                    if self.selection.on_pointer_press(pos, button) {
                        self.exit_requested = true;
                    }
                }
            }
        }

        // If selection was cancelled (e.g., right-click), clear annotations too
        if prev_had_selection && self.global_selection_rect().is_none() {
            self.annotations.clear();
        }
    }

    /// Tool-aware pointer motion handler.
    fn handle_pointer_motion(&mut self, pos: (f64, f64)) {
        // Compute snap rect for this pointer position
        let snap_rect = crate::snap::find_snap_window(
            &self.windows,
            crate::geom::Point::new(pos.0 as i32, pos.1 as i32),
        );

        // Always forward to selection (it needs to track pointer for cursor)
        self.selection.on_pointer_motion(pos, snap_rect);

        // Only forward to annotations when selection is confirmed
        if self.selection.selection.is_confirmed() {
            match self.tool {
                Tool::Move => {}
                Tool::Pen | Tool::Line | Tool::Rect | Tool::Ellipse | Tool::Mosaic => {
                    self.annotations.on_pointer_motion(pos);
                }
            }
        }
    }

    /// Tool-aware pointer release handler.
    fn handle_pointer_release(&mut self, pos: (f64, f64), button: u32) {
        // Check whether selection was confirmed BEFORE release processing,
        // so that a PendingSnap→Confirmed transition doesn't forward an
        // unmatched release to annotations.
        let was_confirmed = self.selection.selection.is_confirmed();

        // Always forward to selection
        self.selection.on_pointer_release(pos, button);

        // Only forward to annotations when selection was already confirmed
        if was_confirmed {
            match self.tool {
                Tool::Move => {}
                Tool::Pen | Tool::Line | Tool::Rect | Tool::Ellipse | Tool::Mosaic => {
                    self.annotations.on_pointer_release(pos, button);
                }
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
            let (uploaded, cap_size) = match cap {
                Some(cap) => {
                    tracing::info!("uploading texture for output '{output_name:?}' ({}x{})",
                        cap.width, cap.height);
                    let up = self.gpu.upload_bgra_texture(cap.width, cap.height, cap.stride, &cap.bgra);
                    (up, (cap.width, cap.height))
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
            let ann_vbuf = self.gpu.create_annotation_vertex_buffer();
            let mos_vbuf = self.gpu.create_mosaic_vertex_buffer();

            // Blur is deferred until a mosaic quad actually needs it — see
            // `ensure_blurred_bind_group`.
            self.overlays[idx].wgpu_surface = Some(wgpu_surf);
            self.overlays[idx].bg_bind_group = Some(uploaded.bind_group);
            self.overlays[idx].bg_view = Some(uploaded.view);
            self.overlays[idx].bg_size = Some(cap_size);
            self.overlays[idx].blurred_bind_group = None;
            self.overlays[idx].blur_passes_used = None;
            self.overlays[idx].surface_config = Some(config);
            self.overlays[idx].selection_buffer = Some(sel_buf);
            self.overlays[idx].selection_bind_group = Some(sel_bg);
            self.overlays[idx].selection_vbuf = Some(sel_vbuf);
            self.overlays[idx].annotation_vbuf = Some(ann_vbuf);
            self.overlays[idx].mosaic_vbuf = Some(mos_vbuf);
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

            // Convert to global logical coords
            let global_pos = self.overlays.iter()
                .find(|o| o.layer.wl_surface() == &event.surface)
                .map(|o| (x + o.output_pos.0 as f64, y + o.output_pos.1 as f64))
                .unwrap_or((x, y));

            // Convert to per-output local coords for egui
            let output = self.overlays.iter()
                .find(|o| o.layer.wl_surface() == &event.surface);
            let egui_pos = output
                .map(|_| (x, y))
                .unwrap_or(global_pos);

            match &event.kind {
                PointerEventKind::Press { button, time, .. } => {
                    // Decide ownership on Press: if pointer is over toolbar → egui, else → overlay
                    let over_toolbar = self.compute_toolbar_rect().map_or(false, |r| {
                        let p = crate::geom::Point::new(global_pos.0 as i32, global_pos.1 as i32);
                        r.contains(p)
                    });

                    if over_toolbar {
                        let egui_button = match *button {
                            0x110 => egui::PointerButton::Primary,
                            0x111 => egui::PointerButton::Secondary,
                            0x112 => egui::PointerButton::Middle,
                            _ => egui::PointerButton::Primary,
                        };
                        self.egui.on_pointer_button(egui_pos, egui_button, true);
                        self.pointer_owner = PointerOwner::Egui;
                    } else {
                        self.handle_pointer_press(global_pos, *button, *time);
                        self.pointer_owner = PointerOwner::Overlay;
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    match self.pointer_owner {
                        PointerOwner::Egui => {
                            let egui_button = match *button {
                                0x110 => egui::PointerButton::Primary,
                                0x111 => egui::PointerButton::Secondary,
                                0x112 => egui::PointerButton::Middle,
                                _ => egui::PointerButton::Primary,
                            };
                            self.egui.on_pointer_button(egui_pos, egui_button, false);
                        }
                        PointerOwner::Overlay | PointerOwner::None => {
                            self.handle_pointer_release(global_pos, *button);
                        }
                    }
                    self.pointer_owner = PointerOwner::None;
                }
                PointerEventKind::Motion { .. } => {
                    // Always feed motion to egui for hover effects, but only
                    // route to overlay if it owns the drag (or no drag active).
                    self.egui.on_pointer_move(egui_pos);
                    if self.pointer_owner != PointerOwner::Egui {
                        self.handle_pointer_motion(global_pos);
                    }
                }
                PointerEventKind::Enter { .. } | PointerEventKind::Leave { .. } => {}
                PointerEventKind::Axis { .. } => {}
            }

            self.update_cursor(conn, global_pos);
        }

        // Mark dirty for re-render
        self.dirty = true;
    }
}

impl KeyboardHandler for OverlayState {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {}
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {}
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        if event.keysym == Keysym::Escape {
            // If an annotation is selected, deselect it first
            if self.annotations.has_selection() {
                self.annotations.deselect_all();
                self.dirty = true;
            } else if self.selection.on_escape() {
                tracing::info!("Esc pressed, exiting overlay");
                self.exit_requested = true;
            } else {
                tracing::info!("Esc pressed, selection cleared");
                // Also clear annotations when selection is cleared
                self.annotations.clear();
                self.dirty = true;
            }
        } else if event.keysym == Keysym::Return {
            self.confirm_and_exit();
        } else if event.keysym == Keysym::Delete {
            self.annotations.on_delete();
            self.dirty = true;
        }
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, mods: Modifiers, _: RawModifiers, _: u32) {
        self.modifiers = mods;
    }
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
pub fn run(
    dbus_conn: zbus::Connection,
    windows: Vec<crate::kwin::windows::WindowInfo>,
) -> Result<()> {
    run_inner(dbus_conn, windows, None)
}

/// Run the overlay event loop with an auto-exit timeout.
pub fn run_with_timeout(
    dbus_conn: zbus::Connection,
    windows: Vec<crate::kwin::windows::WindowInfo>,
    timeout: std::time::Duration,
) -> Result<()> {
    run_inner(dbus_conn, windows, Some(timeout))
}

fn run_inner(
    dbus_conn: zbus::Connection,
    windows: Vec<crate::kwin::windows::WindowInfo>,
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
    let blur = crate::blur::DualBlur::new(gpu.device.clone(), gpu.queue.clone())?;

    let mut state = OverlayState {
        registry_state: RegistryState::new(&globals),
        compositor, output_state, seat_state, layer_shell, shm_state: shm,
        keyboard: None, themed_pointer: None, overlays: Vec::new(), gpu, captured: Vec::new(),
        exit_requested: false, display_ptr,
        selection: SelectionState::new(),
        blur,
        current_cursor: None,
        dirty: false,
        annotations: AnnotationState::new(),
        tool: Tool::Move,
        blur_passes: 3,
        egui: EguiState::new(1.0),
        pointer_owner: PointerOwner::None,
        windows,
        last_click: None,
        modifiers: Modifiers::default(),
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

    // Store individual screen rects for dead-zone-aware clamping
    state.selection.screen_rects = state.output_state.outputs()
        .filter_map(|o| state.output_state.info(&o))
        .filter_map(|info| {
            let (x, y) = info.logical_position.unwrap_or((0, 0));
            let (w, h) = info.logical_size.unwrap_or((1920, 1080));
            Some(Rect::new(x, y, w as i32, h as i32))
        })
        .collect();
    tracing::info!("screen rects: {:?}", state.selection.screen_rects);

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
