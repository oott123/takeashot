use crate::geom::{Point, Rect};

/// Which handle is being interacted with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

/// Cursor shape to display, derived from selection state + pointer position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    Crosshair,
    Move,
    ResizeNWSE,
    ResizeNESW,
    ResizeNS,
    ResizeEW,
}

/// Active drag operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragOp {
    Creating,
    Moving,
    Resizing(Handle),
    Extending(Handle),
}

/// Selection state: None → Pending (window snap preview, M6) → Confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    None,
    Pending { rect: Rect },
    Confirmed { rect: Rect },
}

impl Selection {
    pub fn rect(&self) -> Option<&Rect> {
        match self {
            Selection::None => None,
            Selection::Pending { rect } | Selection::Confirmed { rect } => Some(rect),
        }
    }

    pub fn is_confirmed(&self) -> bool {
        matches!(self, Selection::Confirmed { .. })
    }
}

/// Pure selection state machine. No rendering, no Wayland — just logic.
pub struct SelectionState {
    pub selection: Selection,
    drag: Option<DragOp>,
    last_pointer: Option<(f64, f64)>,
    drag_start_rect: Option<Rect>,
    drag_start_pos: Option<(f64, f64)>,
}

/// Result of a confirm action (Enter key or double-click).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmAction {
    /// No selection to confirm.
    NoSelection,
    /// Selection was confirmed; the caller should capture/copy.
    Confirmed { rect: Rect },
}

impl SelectionState {
    pub fn new() -> Self {
        Self {
            selection: Selection::None,
            drag: None,
            last_pointer: None,
            drag_start_rect: None,
            drag_start_pos: None,
        }
    }

    /// Hit-test the 8 handles of a confirmed selection.
    /// Returns the handle under the pointer, if any.
    pub fn handle_at(rect: &Rect, pos: Point, tolerance: i32) -> Option<Handle> {
        let l = rect.left();
        let t = rect.top();
        let r = rect.right();
        let b = rect.bottom();
        let mx = (l + r) / 2;
        let my = (t + b) / 2;

        let handles = [
            (Handle::TopLeft, l, t),
            (Handle::Top, mx, t),
            (Handle::TopRight, r, t),
            (Handle::Left, l, my),
            (Handle::Right, r, my),
            (Handle::BottomLeft, l, b),
            (Handle::Bottom, mx, b),
            (Handle::BottomRight, r, b),
        ];

        for (handle, hx, hy) in &handles {
            if (pos.x - hx).unsigned_abs() <= tolerance as u32 && (pos.y - hy).unsigned_abs() <= tolerance as u32 {
                return Some(*handle);
            }
        }
        None
    }

    /// Determine which "virtual handle" to use when extending from outside the selection.
    /// Based on the 9-grid position of the click relative to the rect.
    fn extension_handle(rect: &Rect, pos: Point) -> Handle {
        let left = pos.x < rect.left();
        let right = pos.x >= rect.right();
        let above = pos.y < rect.top();
        let below = pos.y >= rect.bottom();

        match (left, right, above, below) {
            // Horizontal inside, extending vertically
            (false, false, true, false) => Handle::Top,
            (false, false, false, true) => Handle::Bottom,
            // Vertical inside, extending horizontally
            (true, false, false, false) => Handle::Left,
            (false, true, false, false) => Handle::Right,
            // Diagonal
            (true, false, true, false) => Handle::TopLeft,
            (false, true, true, false) => Handle::TopRight,
            (true, false, false, true) => Handle::BottomLeft,
            (false, true, false, true) => Handle::BottomRight,
            // Inside rect (shouldn't happen, but treat as no-op)
            _ => Handle::BottomRight,
        }
    }

    /// Apply a resize operation by moving a handle edge.
    fn resize_with_handle(rect: &Rect, handle: Handle, dx: i32, dy: i32) -> Rect {
        let mut r = *rect;
        match handle {
            Handle::TopLeft | Handle::Left | Handle::BottomLeft => { r.x += dx; r.w -= dx; }
            Handle::TopRight | Handle::Right | Handle::BottomRight => { r.w += dx; }
            Handle::Top | Handle::Bottom => {}
        }
        match handle {
            Handle::TopLeft | Handle::Top | Handle::TopRight => { r.y += dy; r.h -= dy; }
            Handle::BottomLeft | Handle::Bottom | Handle::BottomRight => { r.h += dy; }
            Handle::Left | Handle::Right => {}
        }
        r
    }

    /// Handle a pointer press event.
    /// Returns true if the overlay should exit (right-click with no selection).
    pub fn on_pointer_press(&mut self, pos: (f64, f64), button: u32) -> bool {
        // Right click: cancel selection or exit
        if button == BTN_RIGHT {
            match self.selection {
                Selection::Confirmed { .. } | Selection::Pending { .. } => {
                    self.cancel();
                    return false;
                }
                Selection::None => {
                    return true; // signal exit
                }
            }
        }

        if button != BTN_LEFT {
            return false;
        }

        let p = Point::new(pos.0 as i32, pos.1 as i32);

        match self.selection {
            Selection::None => {
                self.drag = Some(DragOp::Creating);
                self.drag_start_pos = Some(pos);
                self.drag_start_rect = None;
                self.selection = Selection::Pending {
                    rect: Rect::new(p.x, p.y, 0, 0),
                };
            }
            Selection::Confirmed { rect } => {
                // Check handle hit first
                const HANDLE_TOL: i32 = 6;
                if let Some(handle) = Self::handle_at(&rect, p, HANDLE_TOL) {
                    self.drag = Some(DragOp::Resizing(handle));
                    self.drag_start_pos = Some(pos);
                    self.drag_start_rect = Some(rect);
                } else if rect.contains(p) {
                    self.drag = Some(DragOp::Moving);
                    self.drag_start_pos = Some(pos);
                    self.drag_start_rect = Some(rect);
                } else {
                    // External extension: immediately expand to include click point
                    let handle = Self::extension_handle(&rect, p);
                    let expanded = Self::expand_to_include(&rect, p);
                    self.drag = Some(DragOp::Extending(handle));
                    self.drag_start_pos = Some(pos);
                    self.drag_start_rect = Some(expanded);
                    self.selection = Selection::Confirmed { rect: expanded };
                }
            }
            Selection::Pending { .. } => {
                // Pending state is for window snapping (M6), treat as creating
                self.drag = Some(DragOp::Creating);
                self.drag_start_pos = Some(pos);
                self.drag_start_rect = None;
                self.selection = Selection::Pending {
                    rect: Rect::new(p.x, p.y, 0, 0),
                };
            }
        }

        self.last_pointer = Some(pos);
        false
    }

    /// Expand a rect to include a point, using the same directional logic as extension.
    fn expand_to_include(rect: &Rect, p: Point) -> Rect {
        let mut r = *rect;
        if p.x < r.x { r.w += r.x - p.x; r.x = p.x; }
        if p.x >= r.right() { r.w = p.x - r.x; }
        if p.y < r.y { r.h += r.y - p.y; r.y = p.y; }
        if p.y >= r.bottom() { r.h = p.y - r.y; }
        r
    }

    /// Handle a pointer motion event.
    pub fn on_pointer_motion(&mut self, pos: (f64, f64)) {
        let prev = match self.last_pointer {
            Some(p) => p,
            None => { self.last_pointer = Some(pos); return; }
        };

        let dx = (pos.0 - prev.0) as i32;
        let dy = (pos.1 - prev.1) as i32;

        match self.drag {
            Some(DragOp::Creating) => {
                if let Selection::Pending { rect } = &mut self.selection {
                    let start = self.drag_start_pos.unwrap();
                    rect.x = start.0 as i32;
                    rect.y = start.1 as i32;
                    rect.w = (pos.0 - start.0) as i32;
                    rect.h = (pos.1 - start.1) as i32;
                }
            }
            Some(DragOp::Moving) => {
                if let Selection::Confirmed { rect } = &mut self.selection {
                    *rect = rect.translate(dx, dy);
                }
            }
            Some(DragOp::Resizing(handle)) | Some(DragOp::Extending(handle)) => {
                if let Some(start_rect) = self.drag_start_rect {
                    let total_dx = (pos.0 - self.drag_start_pos.unwrap().0) as i32;
                    let total_dy = (pos.1 - self.drag_start_pos.unwrap().1) as i32;
                    let new_rect = Self::resize_with_handle(&start_rect, handle, total_dx, total_dy);
                    match &mut self.selection {
                        Selection::Confirmed { rect } | Selection::Pending { rect } => {
                            *rect = new_rect;
                        }
                        Selection::None => {}
                    }
                }
            }
            None => {}
        }

        self.last_pointer = Some(pos);
    }

    /// Handle a pointer release event.
    pub fn on_pointer_release(&mut self, _pos: (f64, f64), button: u32) {
        if button != BTN_LEFT {
            return;
        }

        match self.drag {
            Some(DragOp::Creating) => {
                if let Selection::Pending { rect } = &self.selection {
                    let normalized = rect.normalize();
                    if normalized.w >= 2 && normalized.h >= 2 {
                        self.selection = Selection::Confirmed { rect: normalized };
                    } else {
                        // Too small, cancel
                        self.selection = Selection::None;
                    }
                }
            }
            Some(DragOp::Resizing(_)) | Some(DragOp::Extending(_)) => {
                // Normalize the rect after resize
                if let Selection::Confirmed { rect } = &mut self.selection {
                    *rect = rect.normalize();
                    if rect.is_empty() {
                        self.selection = Selection::None;
                    }
                }
            }
            Some(DragOp::Moving) => {
                // Nothing extra to do
            }
            None => {}
        }

        self.drag = None;
        self.drag_start_rect = None;
        self.drag_start_pos = None;
    }

    /// Cancel: Confirmed → None, Pending → None, None → signal exit.
    /// Returns true if the overlay should exit.
    pub fn on_escape(&mut self) -> bool {
        match self.selection {
            Selection::Confirmed { .. } | Selection::Pending { .. } => {
                self.cancel();
                false
            }
            Selection::None => true, // signal exit
        }
    }

    /// Try to confirm the selection (Enter key).
    pub fn on_enter(&self) -> ConfirmAction {
        match &self.selection {
            Selection::Confirmed { rect } => ConfirmAction::Confirmed { rect: *rect },
            _ => ConfirmAction::NoSelection,
        }
    }

    /// Cancel any selection and drag.
    pub fn cancel(&mut self) {
        self.selection = Selection::None;
        self.drag = None;
        self.drag_start_rect = None;
        self.drag_start_pos = None;
    }

    /// Compute the cursor shape for the current pointer position.
    pub fn cursor_for_position(&self, pos: (f64, f64)) -> CursorShape {
        // During drag, cursor follows the drag operation
        if self.drag.is_some() {
            return match self.drag {
                Some(DragOp::Creating) => CursorShape::Crosshair,
                Some(DragOp::Moving) => CursorShape::Move,
                Some(DragOp::Resizing(h)) | Some(DragOp::Extending(h)) => {
                    handle_cursor(&h)
                }
                None => CursorShape::Crosshair,
            };
        }

        let p = Point::new(pos.0 as i32, pos.1 as i32);

        match &self.selection {
            Selection::None => CursorShape::Crosshair,
            Selection::Pending { .. } => CursorShape::Crosshair,
            Selection::Confirmed { rect } => {
                const HANDLE_TOL: i32 = 6;
                if let Some(h) = Self::handle_at(rect, p, HANDLE_TOL) {
                    handle_cursor(&h)
                } else if rect.contains(p) {
                    CursorShape::Move
                } else {
                    CursorShape::Crosshair
                }
            }
        }
    }
}

fn handle_cursor(h: &Handle) -> CursorShape {
    match h {
        Handle::TopLeft | Handle::BottomRight => CursorShape::ResizeNWSE,
        Handle::TopRight | Handle::BottomLeft => CursorShape::ResizeNESW,
        Handle::Top | Handle::Bottom => CursorShape::ResizeNS,
        Handle::Left | Handle::Right => CursorShape::ResizeEW,
    }
}

/// Mouse button constants (from linux/input-event-codes.h).
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_creates_selection() {
        let mut s = SelectionState::new();
        s.on_pointer_press((100.0, 200.0), BTN_LEFT);
        s.on_pointer_motion((300.0, 400.0));
        s.on_pointer_release((300.0, 400.0), BTN_LEFT);

        assert_eq!(s.selection, Selection::Confirmed {
            rect: Rect::new(100, 200, 200, 200)
        });
    }

    #[test]
    fn drag_right_to_left_normalizes() {
        let mut s = SelectionState::new();
        s.on_pointer_press((300.0, 400.0), BTN_LEFT);
        s.on_pointer_motion((100.0, 200.0));
        s.on_pointer_release((100.0, 200.0), BTN_LEFT);

        assert_eq!(s.selection, Selection::Confirmed {
            rect: Rect::new(100, 200, 200, 200)
        });
    }

    #[test]
    fn too_small_drag_is_cancelled() {
        let mut s = SelectionState::new();
        s.on_pointer_press((100.0, 200.0), BTN_LEFT);
        s.on_pointer_motion((101.0, 201.0));
        s.on_pointer_release((101.0, 201.0), BTN_LEFT);

        assert_eq!(s.selection, Selection::None);
    }

    #[test]
    fn move_selection() {
        let mut s = SelectionState::new();
        s.selection = Selection::Confirmed { rect: Rect::new(100, 100, 200, 200) };

        s.on_pointer_press((150.0, 150.0), BTN_LEFT); // inside rect
        s.on_pointer_motion((160.0, 170.0));
        s.on_pointer_release((160.0, 170.0), BTN_LEFT);

        assert_eq!(s.selection, Selection::Confirmed {
            rect: Rect::new(110, 120, 200, 200)
        });
    }

    #[test]
    fn resize_with_bottom_right_handle() {
        let mut s = SelectionState::new();
        s.selection = Selection::Confirmed { rect: Rect::new(100, 100, 200, 200) };

        // Click on bottom-right handle (300, 300)
        s.on_pointer_press((300.0, 300.0), BTN_LEFT);
        s.on_pointer_motion((350.0, 350.0));
        s.on_pointer_release((350.0, 350.0), BTN_LEFT);

        assert_eq!(s.selection, Selection::Confirmed {
            rect: Rect::new(100, 100, 250, 250)
        });
    }

    #[test]
    fn extend_vertically_from_below() {
        let mut s = SelectionState::new();
        s.selection = Selection::Confirmed { rect: Rect::new(100, 100, 200, 200) };

        // Click below the rect (200, 400) — immediately expands bottom to 400
        s.on_pointer_press((200.0, 400.0), BTN_LEFT);
        // After press: rect bottom is now 400, so h=300 (400-100)
        match s.selection {
            Selection::Confirmed { rect } => {
                assert_eq!(rect.h, 300);
                assert_eq!(rect.y, 100);
            }
            _ => panic!("expected Confirmed after press"),
        }

        // Drag further down to 450 — extends another 50 from drag start
        s.on_pointer_motion((200.0, 450.0));
        s.on_pointer_release((200.0, 450.0), BTN_LEFT);
        match s.selection {
            Selection::Confirmed { rect } => {
                assert_eq!(rect.h, 350); // 300 + 50
                assert_eq!(rect.y, 100);
            }
            _ => panic!("expected Confirmed"),
        }
    }

    #[test]
    fn extend_horizontally_from_right() {
        let mut s = SelectionState::new();
        s.selection = Selection::Confirmed { rect: Rect::new(100, 100, 200, 200) };

        // Click to the right (400, 200) — immediately expands right to 400
        s.on_pointer_press((400.0, 200.0), BTN_LEFT);
        match s.selection {
            Selection::Confirmed { rect } => {
                assert_eq!(rect.w, 300); // 400 - 100
                assert_eq!(rect.x, 100);
            }
            _ => panic!("expected Confirmed after press"),
        }

        s.on_pointer_motion((450.0, 200.0));
        s.on_pointer_release((450.0, 200.0), BTN_LEFT);
        match s.selection {
            Selection::Confirmed { rect } => {
                assert_eq!(rect.w, 350); // 300 + 50
                assert_eq!(rect.x, 100);
            }
            _ => panic!("expected Confirmed"),
        }
    }

    #[test]
    fn extend_diagonally() {
        let mut s = SelectionState::new();
        s.selection = Selection::Confirmed { rect: Rect::new(100, 100, 200, 200) };

        // Click bottom-right (400, 400) — immediately expands to include that point
        s.on_pointer_press((400.0, 400.0), BTN_LEFT);
        match s.selection {
            Selection::Confirmed { rect } => {
                assert_eq!(rect.w, 300); // 400 - 100
                assert_eq!(rect.h, 300); // 400 - 100
            }
            _ => panic!("expected Confirmed after press"),
        }

        s.on_pointer_motion((450.0, 450.0));
        s.on_pointer_release((450.0, 450.0), BTN_LEFT);
        match s.selection {
            Selection::Confirmed { rect } => {
                assert_eq!(rect.w, 350); // 300 + 50
                assert_eq!(rect.h, 350); // 300 + 50
            }
            _ => panic!("expected Confirmed"),
        }
    }

    #[test]
    fn right_click_cancels() {
        let mut s = SelectionState::new();
        s.selection = Selection::Confirmed { rect: Rect::new(100, 100, 200, 200) };

        let should_exit = s.on_pointer_press((150.0, 150.0), BTN_RIGHT);
        assert!(!should_exit);
        assert_eq!(s.selection, Selection::None);
    }

    #[test]
    fn right_click_no_selection_exits() {
        let mut s = SelectionState::new();
        let should_exit = s.on_pointer_press((100.0, 100.0), BTN_RIGHT);
        assert!(should_exit);
    }

    #[test]
    fn escape_cancels_selection() {
        let mut s = SelectionState::new();
        s.selection = Selection::Confirmed { rect: Rect::new(100, 100, 200, 200) };

        let should_exit = s.on_escape();
        assert!(!should_exit);
        assert_eq!(s.selection, Selection::None);

        // Second escape signals exit
        let should_exit = s.on_escape();
        assert!(should_exit);
    }

    #[test]
    fn cursor_crosshair_when_no_selection() {
        let s = SelectionState::new();
        assert_eq!(s.cursor_for_position((100.0, 100.0)), CursorShape::Crosshair);
    }

    #[test]
    fn cursor_move_inside_confirmed() {
        let mut s = SelectionState::new();
        s.selection = Selection::Confirmed { rect: Rect::new(100, 100, 200, 200) };
        assert_eq!(s.cursor_for_position((200.0, 200.0)), CursorShape::Move);
    }

    #[test]
    fn cursor_resize_on_handle() {
        let mut s = SelectionState::new();
        s.selection = Selection::Confirmed { rect: Rect::new(100, 100, 200, 200) };
        // Top-left handle at (100, 100)
        assert_eq!(s.cursor_for_position((100.0, 100.0)), CursorShape::ResizeNWSE);
        // Bottom-right handle at (300, 300)
        assert_eq!(s.cursor_for_position((300.0, 300.0)), CursorShape::ResizeNWSE);
        // Top-right handle at (300, 100)
        assert_eq!(s.cursor_for_position((300.0, 100.0)), CursorShape::ResizeNESW);
        // Bottom handle at (200, 300)
        assert_eq!(s.cursor_for_position((200.0, 300.0)), CursorShape::ResizeNS);
        // Right handle at (300, 200)
        assert_eq!(s.cursor_for_position((300.0, 200.0)), CursorShape::ResizeEW);
    }

    #[test]
    fn cursor_crosshair_outside_confirmed() {
        let mut s = SelectionState::new();
        s.selection = Selection::Confirmed { rect: Rect::new(100, 100, 200, 200) };
        assert_eq!(s.cursor_for_position((50.0, 50.0)), CursorShape::Crosshair);
    }
}
