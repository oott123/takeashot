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
use crate::annotation::AnnotationAction;
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
    surface_config: Option<wgpu::SurfaceConfiguration>,
    selection_buffer: Option<wgpu::Buffer>,
    selection_bind_group: Option<wgpu::BindGroup>,
    /// Pre-allocated vertex buffer for selection geometry (handles + border).
    selection_vbuf: Option<wgpu::Buffer>,
    /// Pre-allocated vertex buffer for annotation geometry.
    annotation_vbuf: Option<wgpu::Buffer>,
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
    /// Current cursor shape (to avoid redundant set_cursor calls).
    current_cursor: Option<CursorShape>,
    /// Whether the display needs re-rendering (selection changed, etc.)
    dirty: bool,
    /// Annotation state (shapes, drawing, editing).
    annotations: AnnotationState,
    /// Currently active tool.
    tool: Tool,
    /// Egui state for toolbar rendering.
    egui: EguiState,
    /// Which subsystem owns the current pointer drag.
    pointer_owner: PointerOwner,
    /// Window list from KWin (for snap matching). Empty if fetch failed.
    windows: Vec<WindowInfo>,
    /// Last left-click time and position for double-click detection.
    last_click: Option<(u32, (f64, f64))>,
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
            selection_vbuf: None, annotation_vbuf: None,
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

    /// Compute the toolbar bounding rect in global logical coordinates.
    /// Uses the same positioning logic as render_all().
    fn compute_toolbar_rect(&self) -> Option<Rect> {
        let global_rect = self.global_selection_rect()?;
        // Find the output that contains the toolbar's preferred position
        let tb_x = (global_rect.right() - 360).max(0);
        let tb_y = global_rect.bottom() + 4;
        let overlay = self.overlays.iter().find(|o| {
            tb_x >= o.output_pos.0 && tb_x < o.output_pos.0 + o.width as i32 &&
            tb_y >= o.output_pos.1 && tb_y < o.output_pos.1 + o.height as i32
        })?;
        crate::ui::toolbar::toolbar_rect(
            Some(global_rect),
            overlay.output_pos,
            (overlay.width, overlay.height),
        )
    }

    /// Update selection uniform buffers + vertex buffers, then render all overlays.
    fn render_all(&mut self) {
        // Determine which output should display the toolbar.
        // The toolbar is positioned below-right of the selection, so find the output
        // that contains the toolbar's preferred position.
        let global_rect = self.global_selection_rect();
        let toolbar_output_idx = global_rect.and_then(|sel| {
            let tb_x = (sel.right() - 360).max(0);
            let tb_y = sel.bottom() + 4;
            self.overlays.iter().position(|o| {
                tb_x >= o.output_pos.0 && tb_x < o.output_pos.0 + o.width as i32 &&
                tb_y >= o.output_pos.1 && tb_y < o.output_pos.1 + o.height as i32
            })
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

        if let Some(new_tool) = self.egui.run_ui(
            &self.gpu.device,
            &self.gpu.queue,
            self.tool,
            global_rect,
            tb_output_pos,
            tb_output_size,
        ) {
            if new_tool != self.tool {
                tracing::info!("tool changed: {:?} → {:?}", self.tool, new_tool);
                // Deselect annotation when switching away from AnnotationEdit
                if matches!(self.tool, Tool::AnnotationEdit) {
                    self.annotations.deselect_all();
                }
                self.tool = new_tool;
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

                // Acquire surface texture once
                let st = match self.gpu.acquire_surface_texture(surface, config) {
                    Ok(Some(st)) => st,
                    Ok(None) => continue,
                    Err(e) => { tracing::warn!("acquire surface failed: {e}"); continue; }
                };
                let view = st.texture.create_view(&wgpu::TextureViewDescriptor::default());

                // Render main passes (screenshot + annotations + handles)
                self.gpu.render_into(&view, bg, sel_bg, sel_verts, ann_verts_opt);

                // Render egui toolbar on this output only if it's the toolbar output
                if toolbar_output_idx == Some(idx) {
                    self.egui.paint(&self.gpu.device, &self.gpu.queue, &view, (config.width, config.height));
                }

                st.present();
            }
        }

        self.dirty = false;
    }

    /// Update cursor shape based on tool + selection state + pointer position.
    fn update_cursor(&mut self, conn: &Connection, pos: (f64, f64)) {
        let shape = match self.tool {
            Tool::Move => self.selection.cursor_for_position(pos),
            Tool::AnnotationEdit => {
                let sel_rect = self.selection.selection.rect();
                self.annotations.cursor_for_position(pos, Tool::AnnotationEdit, sel_rect)
                    .unwrap_or_else(|| self.selection.cursor_for_position(pos))
            }
            Tool::Pen | Tool::Line | Tool::Rect | Tool::Ellipse => {
                // Inside confirmed selection: crosshair; outside: let selection decide
                let inside = self.selection.selection.rect().map_or(false, |r| {
                    let p = crate::geom::Point::new(pos.0 as i32, pos.1 as i32);
                    r.contains(p)
                });
                if inside && self.selection.selection.is_confirmed() {
                    CursorShape::Crosshair
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
    /// copy to clipboard, and set exit_requested.
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
                    &self.gpu, &output_infos, &self.captured, &self.annotations, rect,
                ) {
                    Ok(img) => {
                        let mut png_buf = Vec::new();
                        let encoder = image::codecs::png::PngEncoder::new(&mut png_buf);
                        match image::ImageEncoder::write_image(
                            encoder,
                            img.as_raw(),
                            img.width(),
                            img.height(),
                            image::ExtendedColorType::Rgba8,
                        ) {
                            Ok(()) => {
                                if let Err(e) = crate::clipboard::copy_to_clipboard(&png_buf) {
                                    tracing::error!("clipboard copy failed: {e:#}");
                                }
                            }
                            Err(e) => tracing::error!("PNG encode failed: {e:#}"),
                        }
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
            Tool::AnnotationEdit => {
                let sel_rect = self.selection.selection.rect().copied();
                let action = self.annotations.on_pointer_press(pos, button, Tool::AnnotationEdit, sel_rect);
                if action == AnnotationAction::None {
                    // Annotation didn't consume — fall through to selection
                    if self.selection.on_pointer_press(pos, button) {
                        self.exit_requested = true;
                    }
                }
            }
            Tool::Pen | Tool::Line | Tool::Rect | Tool::Ellipse => {
                // Check if pointer is inside confirmed selection
                let inside = self.selection.selection.rect().map_or(false, |r| {
                    let p = crate::geom::Point::new(pos.0 as i32, pos.1 as i32);
                    r.contains(p)
                });
                if inside && self.selection.selection.is_confirmed() {
                    let sel_rect = self.selection.selection.rect().copied();
                    self.annotations.on_pointer_press(pos, button, self.tool, sel_rect);
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

        // Also forward to annotations if drawing or editing
        match self.tool {
            Tool::Move => {}
            Tool::AnnotationEdit | Tool::Pen | Tool::Line | Tool::Rect | Tool::Ellipse => {
                self.annotations.on_pointer_motion(pos);
            }
        }
    }

    /// Tool-aware pointer release handler.
    fn handle_pointer_release(&mut self, pos: (f64, f64), button: u32) {
        // Always forward to selection
        self.selection.on_pointer_release(pos, button);

        // Also forward to annotations if drawing or editing
        match self.tool {
            Tool::Move => {}
            Tool::AnnotationEdit | Tool::Pen | Tool::Line | Tool::Rect | Tool::Ellipse => {
                self.annotations.on_pointer_release(pos, button);
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
            let ann_vbuf = self.gpu.create_annotation_vertex_buffer();

            self.overlays[idx].wgpu_surface = Some(wgpu_surf);
            self.overlays[idx].bg_bind_group = Some(bg_bind_group);
            self.overlays[idx].surface_config = Some(config);
            self.overlays[idx].selection_buffer = Some(sel_buf);
            self.overlays[idx].selection_bind_group = Some(sel_bg);
            self.overlays[idx].selection_vbuf = Some(sel_vbuf);
            self.overlays[idx].annotation_vbuf = Some(ann_vbuf);
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

    let mut state = OverlayState {
        registry_state: RegistryState::new(&globals),
        compositor, output_state, seat_state, layer_shell, shm_state: shm,
        keyboard: None, themed_pointer: None, overlays: Vec::new(), gpu, captured: Vec::new(),
        exit_requested: false, display_ptr,
        selection: SelectionState::new(),
        current_cursor: None,
        dirty: false,
        annotations: AnnotationState::new(),
        tool: Tool::Move,
        egui: EguiState::new(1.0),
        pointer_owner: PointerOwner::None,
        windows,
        last_click: None,
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

    // Compute total screen bounds from overlay positions + logical sizes
    let bounds: Option<Rect> = state.output_state.outputs()
        .filter_map(|o| state.output_state.info(&o))
        .filter_map(|info| {
            let (x, y) = info.logical_position.unwrap_or((0, 0));
            let (w, h) = info.logical_size.unwrap_or((1920, 1080));
            Some(Rect::new(x, y, w as i32, h as i32))
        })
        .reduce(|a, r| Rect::new(
            a.x.min(r.x), a.y.min(r.y),
            a.right().max(r.right()) - a.x.min(r.x),
            a.bottom().max(r.bottom()) - a.y.min(r.y),
        ));
    state.selection.bounds = bounds;
    tracing::info!("selection bounds: {bounds:?}");

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
