use crate::geom::Rect;

/// Available tools in the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Move,
    Pen,
    Line,
    Rect,
    Ellipse,
    Mosaic,
}

impl Tool {
    /// All tools in toolbar order.
    pub const ALL: [Tool; 6] = [
        Tool::Move,
        Tool::Pen,
        Tool::Line,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Mosaic,
    ];

    /// Display label for the tool.
    pub fn label(&self) -> &'static str {
        match self {
            Tool::Move => "\u{2195} Move",          // ↕
            Tool::Pen => "\u{270F} Pen",             // ✏
            Tool::Line => "\u{2571} Line",           // ╱
            Tool::Rect => "\u{25AD} Rect",           // ▭
            Tool::Ellipse => "\u{2B2D} Ellipse",     // ⬭
            Tool::Mosaic => "\u{25A3} Mosaic",       // ▣
        }
    }
}

/// Valid range for the mosaic blur-pass slider.
pub const BLUR_PASSES_MIN: u32 = 1;
pub const BLUR_PASSES_MAX: u32 = 8;

/// Toolbar pixel dimensions (width, height).
/// Height accounts for the two rows (tool buttons + tool-specific options).
pub const TOOLBAR_WIDTH: f32 = 368.0;
pub const TOOLBAR_HEIGHT: f32 = 68.0;

/// Compute the toolbar bounding rect in global logical coordinates.
/// Returns None if there is no selection.
///
/// The returned rect accounts for all three possible toolbar positions.
pub fn toolbar_rect(
    selection: Option<Rect>,
    output_pos: (i32, i32),
    output_size: (u32, u32),
) -> Option<Rect> {
    let sel = selection?;
    let screen = Rect::new(0, 0, output_size.0 as i32, output_size.1 as i32);
    let local_sel = sel.translate(-output_pos.0, -output_pos.1);
    let (tw, th) = (TOOLBAR_WIDTH, TOOLBAR_HEIGHT);
    let (tx, ty) = place_toolbar(&local_sel, &screen, (tw, th), 4.0);

    // Convert back to global coords
    let gx = tx as i32 + output_pos.0;
    let gy = ty as i32 + output_pos.1;
    let gw = tw as i32;
    let gh = th as i32;
    Some(Rect::new(gx, gy, gw, gh))
}

/// Key for storing tool change in egui temp data.
static TOOL_CHANGE_KEY: &str = "takeashot_tool_change";

/// Compute the toolbar position relative to the selection rect.
///
/// Position priority (from project-overview.md section 5.1):
/// 1. Below-right of selection (preferred)
/// 2. Above-right of selection (when below has no space)
/// 3. Inside bottom-right of selection (when neither above nor below has space)
///
/// `selection` is in the local coordinate system of the output.
/// `screen` is the output's logical bounds (typically (0, 0, w, h)).
/// `toolbar_size` is (width, height) of the toolbar in logical pixels.
/// `margin` is the gap between selection and toolbar in logical pixels.
///
/// Returns the top-left position of the toolbar in the same coordinate system.
pub fn place_toolbar(
    selection: &Rect,
    screen: &Rect,
    toolbar_size: (f32, f32),
    margin: f32,
) -> (f32, f32) {
    let (tw, th) = toolbar_size;
    let sel_right = selection.right() as f32;
    let sel_bottom = selection.bottom() as f32;
    let sel_top = selection.top() as f32;
    let sel_left = selection.left() as f32;
    let screen_bottom = screen.bottom() as f32;
    let screen_right = screen.right() as f32;

    // Position 1: below-right of selection
    let x = (sel_right - tw).max(screen.x as f32);
    let y = sel_bottom + margin;
    if y + th <= screen_bottom && x + tw <= screen_right {
        return (x, y);
    }

    // Position 2: above-right of selection
    let y = sel_top - margin - th;
    if y >= screen.y as f32 && x + tw <= screen_right {
        return (x, y);
    }

    // Position 3: inside bottom-right of selection
    let x = (sel_right - tw).max(sel_left as f32);
    let y = (sel_bottom - th).max(sel_top as f32);
    (x, y)
}

/// Draw the toolbar using egui. Tool/option changes are written into `ctx.data_mut`
/// and must be drained with `take_tool_change` / `take_blur_passes_change`.
///
/// `selection_rect` is in global logical coordinates.
/// `output_pos` is the output's global position.
/// `output_size` is the output's logical size.
/// `blur_passes` is the current mosaic blur-pass count (shown in the Mosaic options row).
pub fn draw_toolbar(
    ctx: &egui::Context,
    active_tool: Tool,
    blur_passes: u32,
    selection_rect: Option<Rect>,
    output_pos: (i32, i32),
    output_size: (u32, u32),
) {
    let selection = match selection_rect {
        Some(r) => r,
        None => return, // No selection → no toolbar
    };

    // Convert global selection rect to per-output local coords
    let local_sel = selection.translate(-output_pos.0, -output_pos.1);
    let screen = Rect::new(0, 0, output_size.0 as i32, output_size.1 as i32);

    let (tx, ty) = place_toolbar(&local_sel, &screen, (TOOLBAR_WIDTH, TOOLBAR_HEIGHT), 4.0);

    // egui coordinates are in logical pixels, same as our local_sel coordinates
    let egui_pos = egui::Pos2::new(tx, ty);

    // Create a floating area for the toolbar
    let area = egui::Area::new(egui::Id::new("takeashot_toolbar"))
        .fixed_pos(egui_pos)
        .order(egui::Order::Foreground)
        .interactable(true);

    area.show(ctx, |ui| {
        // Toolbar container: white background with border
        egui::Frame::NONE
            .fill(egui::Color32::WHITE)
            .stroke(egui::Stroke::new(1.0f32, egui::Color32::BLACK))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(4i8, 2i8))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);

                    // Row 1: tool buttons
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);

                        for &tool in &Tool::ALL {
                            let is_selected = tool == active_tool;
                            let label = tool.label();

                            let (rect, response) = ui.allocate_at_least(
                                egui::vec2(48.0, 28.0),
                                egui::Sense::click(),
                            );

                            // Draw button background
                            if ui.is_rect_visible(rect) {
                                let bg_color = if is_selected {
                                    egui::Color32::from_rgb(0x46, 0x87, 0xDB) // KDE blue
                                } else if response.hovered() {
                                    egui::Color32::from_rgb(0xE0, 0xE0, 0xE0)
                                } else {
                                    egui::Color32::WHITE
                                };
                                ui.painter().rect_filled(rect, 2.0, bg_color);

                                let text_color = if is_selected {
                                    egui::Color32::WHITE
                                } else {
                                    egui::Color32::BLACK
                                };
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    label,
                                    egui::FontId::proportional(11.0),
                                    text_color,
                                );
                            }

                            // Handle click
                            if response.clicked() && !is_selected {
                                ctx.data_mut(|data| data.insert_temp(egui::Id::new(TOOL_CHANGE_KEY), tool));
                            }
                        }
                    });

                    // Row 2: tool-specific options
                    draw_tool_options_row(ui, ctx, active_tool, blur_passes);
                });
            });
    });
}

/// Render the second row of tool-specific options. Always allocates the row's
/// full height so the toolbar frame stays a constant size regardless of tool.
fn draw_tool_options_row(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    active_tool: Tool,
    blur_passes: u32,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
        // Reserve a fixed row height so the toolbar doesn't resize per tool.
        ui.set_min_height(26.0);

        match active_tool {
            Tool::Mosaic => {
                ui.label(
                    egui::RichText::new("Blur")
                        .color(egui::Color32::BLACK)
                        .size(11.0),
                );
                let mut val = blur_passes.clamp(BLUR_PASSES_MIN, BLUR_PASSES_MAX);
                let slider = egui::Slider::new(&mut val, BLUR_PASSES_MIN..=BLUR_PASSES_MAX)
                    .integer()
                    .show_value(true);
                let response = ui.add(slider);
                if response.changed() && val != blur_passes {
                    ctx.data_mut(|data| {
                        data.insert_temp(egui::Id::new(BLUR_PASSES_CHANGE_KEY), val)
                    });
                }
            }
            _ => {
                // No options for other tools yet. Placeholder keeps the row the same size.
                ui.label(" ");
            }
        }
    });
}

/// Extract and clear the tool change from egui context data.
pub fn take_tool_change(ctx: &egui::Context) -> Option<Tool> {
    ctx.data_mut(|data| data.remove_temp(egui::Id::new(TOOL_CHANGE_KEY)))
}

/// Key for storing blur-pass slider changes in egui temp data.
static BLUR_PASSES_CHANGE_KEY: &str = "takeashot_blur_passes_change";

/// Extract and clear the blur-passes change from egui context data.
pub fn take_blur_passes_change(ctx: &egui::Context) -> Option<u32> {
    ctx.data_mut(|data| data.remove_temp(egui::Id::new(BLUR_PASSES_CHANGE_KEY)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Rect {
        Rect::new(0, 0, 1920, 1080)
    }

    fn toolbar_size() -> (f32, f32) {
        (300.0, 40.0)
    }

    #[test]
    fn toolbar_below_right() {
        // Selection in the middle of the screen → toolbar below-right
        let sel = Rect::new(100, 100, 400, 300);
        let (x, y) = place_toolbar(&sel, &screen(), toolbar_size(), 4.0);
        // x = sel_right - tw = 500 - 300 = 200
        assert_eq!(x, 200.0);
        // y = sel_bottom + margin = 400 + 4 = 404
        assert_eq!(y, 404.0);
    }

    #[test]
    fn toolbar_above_right_when_no_space_below() {
        // Selection near bottom of screen
        let sel = Rect::new(100, 1000, 400, 50);
        let (x, y) = place_toolbar(&sel, &screen(), toolbar_size(), 4.0);
        // x = sel_right - tw = 500 - 300 = 200
        assert_eq!(x, 200.0);
        // y = sel_top - margin - th = 1000 - 4 - 40 = 956
        assert_eq!(y, 956.0);
    }

    #[test]
    fn toolbar_inside_when_no_space_above_or_below() {
        // Selection spans almost the full screen height
        let sel = Rect::new(100, 10, 400, 1060);
        let (x, y) = place_toolbar(&sel, &screen(), toolbar_size(), 4.0);
        // Below: y = 1070 + 4 = 1074, 1074 + 40 = 1114 > 1080 ✗
        // Above: y = 10 - 4 - 40 = -34 < 0 ✗
        // Inside: x = 500 - 300 = 200, y = 1070 - 40 = 1030
        assert_eq!(x, 200.0);
        assert_eq!(y, 1030.0);
    }

    #[test]
    fn toolbar_x_clamped_to_screen() {
        // Selection very narrow at the left
        let sel = Rect::new(0, 100, 50, 300);
        let (x, _y) = place_toolbar(&sel, &screen(), toolbar_size(), 4.0);
        // x = sel_right - tw = 50 - 300 = -250, clamped to 0
        assert_eq!(x, 0.0);
    }

    #[test]
    fn toolbar_inside_x_clamped_to_sel_left() {
        // Very narrow selection, no space above or below
        let sel = Rect::new(0, 10, 50, 1060);
        let (x, _y) = place_toolbar(&sel, &screen(), toolbar_size(), 4.0);
        // Inside: x = (50 - 300).max(0) = 0
        assert_eq!(x, 0.0);
    }
}
