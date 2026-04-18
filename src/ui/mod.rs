pub mod toolbar;

use egui::RawInput;
use std::time::Instant;

/// State for egui integration (no winit — we feed raw input from SCTK events).
pub struct EguiState {
    pub ctx: egui::Context,
    raw_input: RawInput,
    pixels_per_point: f32,
    /// The start time for computing egui's monotonic clock.
    start_time: Instant,
    /// The wgpu renderer, created lazily.
    renderer: Option<egui_wgpu::Renderer>,
    /// Tessellated paint jobs from the last `run_ui()` call.
    paint_jobs: Vec<egui::ClippedPrimitive>,
}

impl EguiState {
    pub fn new(pixels_per_point: f32) -> Self {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(pixels_per_point);

        Self {
            ctx,
            raw_input: RawInput::default(),
            pixels_per_point,
            start_time: Instant::now(),
            renderer: None,
            paint_jobs: Vec::new(),
        }
    }

    /// Initialize the wgpu renderer. Call after device is available.
    pub fn init_renderer(&mut self, device: &wgpu::Device, surface_format: wgpu::TextureFormat) {
        if self.renderer.is_none() {
            self.renderer = Some(egui_wgpu::Renderer::new(device, surface_format, Default::default()));
        }
    }

    /// Update pixels per point (when scale changes).
    pub fn set_pixels_per_point(&mut self, ppf: f32) {
        self.pixels_per_point = ppf;
        self.ctx.set_pixels_per_point(ppf);
    }

    /// Feed a pointer move event.
    pub fn on_pointer_move(&mut self, pos: (f64, f64)) {
        self.raw_input.events.push(egui::Event::PointerMoved(egui::Pos2::new(pos.0 as f32, pos.1 as f32)));
    }

    /// Feed a pointer button event.
    pub fn on_pointer_button(&mut self, pos: (f64, f64), button: egui::PointerButton, pressed: bool) {
        self.raw_input.events.push(egui::Event::PointerButton {
            pos: egui::Pos2::new(pos.0 as f32, pos.1 as f32),
            button,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
    }

    /// Run the egui UI for one frame AND upload textures immediately.
    /// Returns the tool change (if any).
    ///
    /// After calling this, call `paint()` for each output that should render the toolbar.
    pub fn run_ui(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        active_tool: toolbar::Tool,
        selection_rect: Option<crate::geom::Rect>,
        output_pos: (i32, i32),
        output_size: (u32, u32),
    ) -> Option<toolbar::Tool> {
        // egui expects time as a monotonically increasing value in seconds.
        self.raw_input.time = Some(self.start_time.elapsed().as_secs_f64());
        self.raw_input.screen_rect = Some(egui::Rect::from_min_max(
            egui::Pos2::ZERO,
            egui::Pos2::new(
                output_size.0 as f32,
                output_size.1 as f32,
            ),
        ));

        // ctx.run_ui() processes input, runs the UI closure, AND ends the frame.
        let full_output = self.ctx.run_ui(self.raw_input.take(), |ctx| {
            toolbar::draw_toolbar(ctx, active_tool, selection_rect, output_pos, output_size)
        });

        // Tessellate shapes into paint jobs
        self.paint_jobs = self.ctx.tessellate(full_output.shapes, self.pixels_per_point);

        // Upload textures immediately — must happen before paint() and every frame,
        // even when paint() won't be called (otherwise deltas accumulate and break).
        if let Some(renderer) = &mut self.renderer {
            for (id, image_delta) in &full_output.textures_delta.set {
                renderer.update_texture(device, queue, *id, image_delta);
            }
            for id in &full_output.textures_delta.free {
                renderer.free_texture(id);
            }
        }

        toolbar::take_tool_change(&self.ctx)
    }

    /// Paint the egui UI into a texture view.
    /// Call after `run_ui()`, for the output that should display the toolbar.
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        surface_size: (u32, u32),
    ) {
        let renderer = match &mut self.renderer {
            Some(r) => r,
            None => return,
        };
        if self.paint_jobs.is_empty() {
            return;
        }

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [surface_size.0, surface_size.1],
            pixels_per_point: self.pixels_per_point,
        };

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui paint encoder"),
        });

        renderer.update_buffers(device, queue, &mut encoder, &self.paint_jobs, &screen_descriptor);

        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            renderer.render(&mut render_pass.forget_lifetime(), &self.paint_jobs, &screen_descriptor);
        }

        queue.submit(std::iter::once(encoder.finish()));
    }
}