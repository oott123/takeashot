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
    /// Press on snap preview — will become Creating on drag or Confirmed on release.
    PendingSnap { rect: Rect },
    Moving,
    Resizing(Handle),
    Extending(Handle),
}

/// Selection state: None → Pending (window snap preview) → Confirmed.
/// Creating is the drag-to-select intermediate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    None,
    Pending { rect: Rect },
    Creating { rect: Rect },
    Confirmed { rect: Rect },
}

impl Selection {
    pub fn rect(&self) -> Option<&Rect> {
        match self {
            Selection::None => None,
            Selection::Pending { rect }
            | Selection::Creating { rect }
            | Selection::Confirmed { rect } => Some(rect),
        }
    }

    pub fn is_confirmed(&self) -> bool {
        matches!(self, Selection::Confirmed { .. })
    }

    /// Returns true if this state should render resize handles.
    pub fn shows_handles(&self) -> bool {
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
    /// Total screen bounds (union of all outputs). Used to clamp selection on move.
    pub bounds: Option<Rect>,
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
            bounds: None,
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
                Selection::Confirmed { .. } | Selection::Creating { .. } => {
                    self.cancel();
                    return false;
                }
                Selection::Pending { .. } => {
                    // Pending is just a preview — right-click exits directly
                    return true;
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
                self.selection = Selection::Creating {
                    rect: Rect::new(p.x, p.y, 0, 0),
                };
            }
            Selection::Pending { rect } => {
                // Press on snap preview — don't confirm yet.
                // Will become Confirmed on release (click) or Creating on motion (drag).
                self.drag = Some(DragOp::PendingSnap { rect });
                self.drag_start_pos = Some(pos);
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
            Selection::Creating { .. } => {
                // Shouldn't happen during normal flow (press starts Creating)
                // but handle gracefully by starting a new creation
                self.drag = Some(DragOp::Creating);
                self.drag_start_pos = Some(pos);
                self.drag_start_rect = None;
                self.selection = Selection::Creating {
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
    /// `snap_rect`: the window under the pointer (for snap preview), if any.
    /// Only used when no drag is active and selection is None/Pending.
    pub fn on_pointer_motion(&mut self, pos: (f64, f64), snap_rect: Option<Rect>) {
        let _prev = match self.last_pointer {
            Some(p) => p,
            None => { self.last_pointer = Some(pos); return; }
        };

        match self.drag {
            Some(DragOp::Creating) => {
                if let Selection::Creating { rect } = &mut self.selection {
                    let start = self.drag_start_pos.unwrap();
                    rect.x = start.0 as i32;
                    rect.y = start.1 as i32;
                    rect.w = (pos.0 - start.0) as i32;
                    rect.h = (pos.1 - start.1) as i32;
                }
            }
            Some(DragOp::PendingSnap { .. }) => {
                // If moved enough from press point, override snap with drag creation
                let start = self.drag_start_pos.unwrap();
                let dx = (pos.0 - start.0).abs();
                let dy = (pos.1 - start.1).abs();
                if dx > 2.0 || dy > 2.0 {
                    let snap_rect = match self.selection {
                        Selection::Pending { rect } => rect,
                        _ => Rect::new(start.0 as i32, start.1 as i32, 0, 0),
                    };
                    self.drag = Some(DragOp::Creating);
                    let p = Point::new(start.0 as i32, start.1 as i32);
                    self.selection = Selection::Creating {
                        rect: Rect::new(p.x, p.y, (pos.0 - start.0) as i32, (pos.1 - start.1) as i32),
                    };
                    // Keep snap_rect available for potential cancellation (not needed now,
                    // but if user presses Escape during PendingSnap, we restore it)
                    let _ = snap_rect;
                }
                // If not moved enough, stay in PendingSnap (still showing snap preview)
            }
            Some(DragOp::Moving) => {
                let bounds = self.bounds;
                let start_rect = match self.drag_start_rect {
                    Some(r) => r,
                    None => { self.last_pointer = Some(pos); return; }
                };
                let start = self.drag_start_pos.unwrap();
                let total_dx = (pos.0 - start.0) as i32;
                let total_dy = (pos.1 - start.1) as i32;
                let moved = start_rect.translate(total_dx, total_dy);
                let clamped = match bounds {
                    Some(b) => {
                        let x = moved.x.clamp(b.x, (b.right() - moved.w).max(b.x));
                        let y = moved.y.clamp(b.y, (b.bottom() - moved.h).max(b.y));
                        Rect { x, y, w: moved.w, h: moved.h }
                    }
                    None => moved,
                };
                if let Selection::Confirmed { rect } = &mut self.selection {
                    *rect = clamped;
                }
            }
            Some(DragOp::Resizing(handle)) | Some(DragOp::Extending(handle)) => {
                if let Some(start_rect) = self.drag_start_rect {
                    let total_dx = (pos.0 - self.drag_start_pos.unwrap().0) as i32;
                    let total_dy = (pos.1 - self.drag_start_pos.unwrap().1) as i32;
                    let new_rect = Self::resize_with_handle(&start_rect, handle, total_dx, total_dy);
                    match &mut self.selection {
                        Selection::Confirmed { rect }
                        | Selection::Creating { rect }
                        | Selection::Pending { rect } => {
                            *rect = new_rect;
                        }
                        Selection::None => {}
                    }
                }
            }
            None => {
                // No drag active — update snap preview
                match self.selection {
                    Selection::None | Selection::Pending { .. } => {
                        self.selection = match snap_rect {
                            Some(r) => Selection::Pending { rect: r },
                            None => Selection::None,
                        };
                    }
                    _ => {}
                }
            }
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
                if let Selection::Creating { rect } = &self.selection {
                    let normalized = rect.normalize();
                    if normalized.w >= 2 && normalized.h >= 2 {
                        self.selection = Selection::Confirmed { rect: normalized };
                    } else {
                        // Too small, cancel
                        self.selection = Selection::None;
                    }
                }
            }
            Some(DragOp::PendingSnap { rect }) => {
                // Click on snap preview → confirm
                self.selection = Selection::Confirmed { rect };
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
            Selection::Confirmed { .. } | Selection::Creating { .. } => {
                self.cancel();
                false
            }
            Selection::Pending { .. } | Selection::None => true, // signal exit
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
                Some(DragOp::Creating) | Some(DragOp::PendingSnap { .. }) => CursorShape::Crosshair,
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
            Selection::Creating { .. } => CursorShape::Crosshair,
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
        s.on_pointer_motion((300.0, 400.0), None);
        s.on_pointer_release((300.0, 400.0), BTN_LEFT);

        assert_eq!(s.selection, Selection::Confirmed {
            rect: Rect::new(100, 200, 200, 200)
        });
    }

    #[test]
    fn drag_right_to_left_normalizes() {
        let mut s = SelectionState::new();
        s.on_pointer_press((300.0, 400.0), BTN_LEFT);
        s.on_pointer_motion((100.0, 200.0), None);
        s.on_pointer_release((100.0, 200.0), BTN_LEFT);

        assert_eq!(s.selection, Selection::Confirmed {
            rect: Rect::new(100, 200, 200, 200)
        });
    }

    #[test]
    fn too_small_drag_is_cancelled() {
        let mut s = SelectionState::new();
        s.on_pointer_press((100.0, 200.0), BTN_LEFT);
        s.on_pointer_motion((101.0, 201.0), None);
        s.on_pointer_release((101.0, 201.0), BTN_LEFT);

        assert_eq!(s.selection, Selection::None);
    }

    #[test]
    fn move_selection() {
        let mut s = SelectionState::new();
        s.selection = Selection::Confirmed { rect: Rect::new(100, 100, 200, 200) };

        s.on_pointer_press((150.0, 150.0), BTN_LEFT); // inside rect
        s.on_pointer_motion((160.0, 170.0), None);
        s.on_pointer_release((160.0, 170.0), BTN_LEFT);

        assert_eq!(s.selection, Selection::Confirmed {
            rect: Rect::new(110, 120, 200, 200)
        });
    }

    #[test]
    fn move_clamped_by_bounds() {
        let mut s = SelectionState::new();
        s.bounds = Some(Rect::new(0, 0, 300, 300));
        // 200x200 rect at (100,100), trying to move left past x=0
        s.selection = Selection::Confirmed { rect: Rect::new(100, 100, 200, 200) };

        s.on_pointer_press((150.0, 150.0), BTN_LEFT);
        // Move 200px left — would put x at -100, but clamped to 0
        s.on_pointer_motion((-50.0, 150.0), None);
        match s.selection {
            Selection::Confirmed { rect } => {
                assert_eq!(rect.x, 0); // clamped
                assert_eq!(rect.w, 200); // size preserved
            }
            _ => panic!("expected Confirmed"),
        }
    }

    #[test]
    fn resize_with_bottom_right_handle() {
        let mut s = SelectionState::new();
        s.selection = Selection::Confirmed { rect: Rect::new(100, 100, 200, 200) };

        // Click on bottom-right handle (300, 300)
        s.on_pointer_press((300.0, 300.0), BTN_LEFT);
        s.on_pointer_motion((350.0, 350.0), None);
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
        s.on_pointer_motion((200.0, 450.0), None);
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

        s.on_pointer_motion((450.0, 200.0), None);
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

        s.on_pointer_motion((450.0, 450.0), None);
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

    // --- Snap preview (Pending) tests ---

    #[test]
    fn snap_preview_on_motion() {
        let mut s = SelectionState::new();
        s.last_pointer = Some((0.0, 0.0)); // Initialize so motion doesn't early-return
        let snap = Rect::new(10, 20, 800, 600);
        s.on_pointer_motion((100.0, 100.0), Some(snap));
        assert_eq!(s.selection, Selection::Pending { rect: snap });
    }

    #[test]
    fn snap_preview_clears_when_no_window() {
        let mut s = SelectionState::new();
        // Start with a snap preview
        s.selection = Selection::Pending { rect: Rect::new(10, 20, 800, 600) };
        s.last_pointer = Some((100.0, 100.0));
        // Move to a position with no window
        s.on_pointer_motion((900.0, 900.0), None);
        assert_eq!(s.selection, Selection::None);
    }

    #[test]
    fn snap_preview_updates_on_different_window() {
        let mut s = SelectionState::new();
        s.last_pointer = Some((0.0, 0.0)); // Initialize
        let snap1 = Rect::new(10, 20, 800, 600);
        s.on_pointer_motion((100.0, 100.0), Some(snap1));
        assert_eq!(s.selection, Selection::Pending { rect: snap1 });

        let snap2 = Rect::new(900, 20, 800, 600);
        s.on_pointer_motion((1000.0, 100.0), Some(snap2));
        assert_eq!(s.selection, Selection::Pending { rect: snap2 });
    }

    #[test]
    fn click_on_pending_confirms() {
        let mut s = SelectionState::new();
        let snap = Rect::new(10, 20, 800, 600);
        s.selection = Selection::Pending { rect: snap };
        s.last_pointer = Some((100.0, 100.0));

        // Press enters PendingSnap state
        let should_exit = s.on_pointer_press((100.0, 100.0), BTN_LEFT);
        assert!(!should_exit);
        // Still Pending (not yet confirmed)
        assert!(matches!(s.selection, Selection::Pending { .. }));

        // Release confirms the snap
        s.on_pointer_release((100.0, 100.0), BTN_LEFT);
        assert_eq!(s.selection, Selection::Confirmed { rect: snap });
    }

    #[test]
    fn drag_from_pending_overrides() {
        let mut s = SelectionState::new();
        let snap = Rect::new(10, 20, 800, 600);
        s.selection = Selection::Pending { rect: snap };
        s.last_pointer = Some((100.0, 100.0));

        // Press on Pending → enters PendingSnap (not yet confirmed)
        s.on_pointer_press((100.0, 100.0), BTN_LEFT);
        // Should still be Pending (waiting for release or drag)
        assert!(matches!(s.selection, Selection::Pending { .. }));

        // Motion past threshold → overrides to Creating
        s.on_pointer_motion((150.0, 150.0), None);
        assert!(matches!(s.selection, Selection::Creating { .. }));

        // Release → Creating resolves to Confirmed with drag rect
        s.on_pointer_release((150.0, 150.0), BTN_LEFT);
        assert!(matches!(s.selection, Selection::Confirmed { .. }));
    }

    #[test]
    fn escape_pending_exits() {
        let mut s = SelectionState::new();
        s.selection = Selection::Pending { rect: Rect::new(10, 20, 800, 600) };

        let should_exit = s.on_escape();
        assert!(should_exit);
    }

    #[test]
    fn cancel_creating() {
        let mut s = SelectionState::new();
        s.selection = Selection::Creating { rect: Rect::new(10, 20, 800, 600) };

        let should_exit = s.on_escape();
        assert!(!should_exit);
        assert_eq!(s.selection, Selection::None);
    }

    #[test]
    fn right_click_pending_exits() {
        let mut s = SelectionState::new();
        s.selection = Selection::Pending { rect: Rect::new(10, 20, 800, 600) };

        let should_exit = s.on_pointer_press((100.0, 100.0), BTN_RIGHT);
        assert!(should_exit);
    }

    #[test]
    fn shows_handles_only_for_confirmed() {
        assert!(!Selection::None.shows_handles());
        assert!(!Selection::Pending { rect: Rect::new(0, 0, 100, 100) }.shows_handles());
        assert!(!Selection::Creating { rect: Rect::new(0, 0, 100, 100) }.shows_handles());
        assert!(Selection::Confirmed { rect: Rect::new(0, 0, 100, 100) }.shows_handles());
    }

    #[test]
    fn creating_drag_to_confirmed() {
        let mut s = SelectionState::new();
        s.on_pointer_press((100.0, 200.0), BTN_LEFT);
        // During drag, selection should be Creating
        assert!(matches!(s.selection, Selection::Creating { .. }));
        s.on_pointer_motion((300.0, 400.0), None);
        s.on_pointer_release((300.0, 400.0), BTN_LEFT);
        // After release, Creating → Confirmed
        assert_eq!(s.selection, Selection::Confirmed {
            rect: Rect::new(100, 200, 200, 200)
        });
    }
}