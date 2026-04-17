use crate::geom::{Point, Rect};
use crate::kwin::windows::WindowInfo;

/// Find the topmost window under the given pointer position.
///
/// Windows are expected to be in stacking order (front to back as returned
/// by KWin's `workspace.stackingOrder`). Returns the frame geometry of the
/// first window whose bounds contain the pointer, or `None` if no window
/// matches.
pub fn find_snap_window(windows: &[WindowInfo], pointer: Point) -> Option<Rect> {
    for w in windows {
        let rect = Rect::new(
            w.x.round() as i32,
            w.y.round() as i32,
            w.width.round() as i32,
            w.height.round() as i32,
        );
        if rect.contains(pointer) {
            return Some(rect);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(x: f64, y: f64, w: f64, h: f64) -> WindowInfo {
        WindowInfo {
            caption: String::new(),
            resource_class: String::new(),
            x, y, width: w, height: h,
        }
    }

    #[test]
    fn no_windows() {
        assert_eq!(find_snap_window(&[], Point::new(100, 100)), None);
    }

    #[test]
    fn pointer_inside_single_window() {
        let windows = vec![win(10.0, 20.0, 800.0, 600.0)];
        let result = find_snap_window(&windows, Point::new(100, 100));
        assert_eq!(result, Some(Rect::new(10, 20, 800, 600)));
    }

    #[test]
    fn pointer_outside_window() {
        let windows = vec![win(10.0, 20.0, 800.0, 600.0)];
        assert_eq!(find_snap_window(&windows, Point::new(5, 5)), None);
    }

    #[test]
    fn stacking_order_front_wins() {
        let windows = vec![
            win(0.0, 0.0, 400.0, 400.0),   // front window
            win(0.0, 0.0, 1920.0, 1080.0), // back window (covers more)
        ];
        let result = find_snap_window(&windows, Point::new(200, 200));
        // Should match the front window (first in list)
        assert_eq!(result, Some(Rect::new(0, 0, 400, 400)));
    }

    #[test]
    fn pointer_in_back_window_only() {
        let windows = vec![
            win(0.0, 0.0, 400.0, 400.0),    // front window
            win(400.0, 0.0, 1520.0, 1080.0), // back window
        ];
        let result = find_snap_window(&windows, Point::new(500, 100));
        assert_eq!(result, Some(Rect::new(400, 0, 1520, 1080)));
    }

    #[test]
    fn pointer_on_edge_not_contained() {
        let windows = vec![win(10.0, 20.0, 800.0, 600.0)];
        // right() = 810, bottom() = 620; edge is exclusive
        assert_eq!(find_snap_window(&windows, Point::new(810, 100)), None);
        assert_eq!(find_snap_window(&windows, Point::new(100, 620)), None);
    }
}
