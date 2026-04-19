pub mod render;

use glam::{Affine2, Vec2};
use crate::geom::Rect;
use crate::selection::CursorShape;

/// Unique identifier for an annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnnotationId(pub usize);

/// A geometric shape that can be drawn as an annotation.
#[derive(Debug, Clone)]
pub enum Shape {
    Pen { points: Vec<Vec2> },
    Line { start: Vec2, end: Vec2 },
    Rect { half_extents: Vec2 },
    Ellipse { radii: Vec2 },
    Mosaic { half_extents: Vec2 },
}

/// An annotation: a shape with a transform, color, and stroke width.
#[derive(Debug, Clone)]
pub struct Annotation {
    pub shape: Shape,
    /// Transform from shape-local space to global logical space.
    pub transform: Affine2,
    pub color: [f32; 4],
    pub stroke_width: f32,
}

/// Tolerance for hit-testing edit handles (logical pixels).
const HANDLE_TOLERANCE: f32 = 14.0;

/// Edit handle kind for rendering and hit-testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditHandle {
    /// Corner handle for scaling (0=TL, 1=TR, 2=BR, 3=BL).
    Corner(usize),
    /// Rotation handle above the top edge center.
    Rotation,
}

/// Position of an edit handle in global logical coordinates.
#[derive(Debug, Clone, Copy)]
pub struct EditHandlePos {
    pub kind: EditHandle,
    pub pos: Vec2,
}

/// Oriented bounding box of an annotation in global logical space.
/// Corners are in order: TL, TR, BR, BL (relative to local/unrotated space).
#[derive(Debug, Clone, Copy)]
pub struct OrientedRect {
    pub corners: [Vec2; 4],
}

impl OrientedRect {
    pub fn center(&self) -> Vec2 {
        (self.corners[0] + self.corners[2]) / 2.0
    }
}

/// Active drag operation during annotation editing.
#[derive(Debug, Clone, Copy)]
enum EditDrag {
    Moving { start_pos: Vec2, start_transform: Affine2 },
    ScalingCorner { corner: usize, start_pos: Vec2, start_transform: Affine2, start_center: Vec2 },
    Rotating { start_pos: Vec2, start_angle: f32, start_transform: Affine2, start_center: Vec2 },
}

/// Result of an annotation pointer event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationAction {
    None,
    Consumed,
    ExitRequested,
}

/// Annotation state machine: manages a list of annotations, drawing, and editing.
pub struct AnnotationState {
    annotations: Vec<Annotation>,
    selected_id: Option<usize>,
    edit_drag: Option<EditDrag>,
    /// The shape currently being drawn (between press and release).
    drawing: Option<Shape>,
    /// Position where the current drawing started.
    draw_start: Option<Vec2>,
    /// Current pointer position during drawing (for computing center of Rect/Ellipse).
    draw_current: Option<Vec2>,
    next_id: usize,
}

/// Default annotation color: red.
const DEFAULT_COLOR: [f32; 4] = [1.0, 0.2, 0.2, 1.0];
/// Default stroke width in logical pixels.
const DEFAULT_STROKE_WIDTH: f32 = 3.0;

/// Mouse button constants (from linux/input-event-codes.h).
const BTN_LEFT: u32 = 0x110;

impl AnnotationState {
    pub fn new() -> Self {
        Self {
            annotations: Vec::new(),
            selected_id: None,
            edit_drag: None,
            drawing: None,
            draw_start: None,
            draw_current: None,
            next_id: 0,
        }
    }

    /// Clear all annotations and reset state.
    pub fn clear(&mut self) {
        self.annotations.clear();
        self.selected_id = None;
        self.edit_drag = None;
        self.drawing = None;
        self.draw_start = None;
        self.draw_current = None;
    }

    /// Deselect any selected annotation.
    pub fn deselect_all(&mut self) {
        self.selected_id = None;
        self.edit_drag = None;
    }

    /// Delete the currently selected annotation.
    pub fn on_delete(&mut self) {
        if let Some(idx) = self.selected_id {
            if idx < self.annotations.len() {
                self.annotations.remove(idx);
                // Adjust selected_id for annotations after the removed one
                self.selected_id = None;
            }
        }
        self.edit_drag = None;
    }

    /// Get all annotations.
    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    /// Get the index of the selected annotation, if any.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected_id
    }

    /// Check if an annotation is selected.
    pub fn has_selection(&self) -> bool {
        self.selected_id.is_some()
    }

    /// Get the in-progress drawing shape (for rendering preview).
    pub fn drawing_shape(&self) -> Option<&Shape> {
        self.drawing.as_ref()
    }

    /// Get the transform for the in-progress drawing shape.
    /// Pen/Line use IDENTITY (points are in global coords).
    /// Rect/Ellipse need translation to the center of the drag.
    pub fn drawing_transform(&self) -> Option<Affine2> {
        match self.drawing.as_ref()? {
            Shape::Pen { .. } | Shape::Line { .. } => Some(Affine2::IDENTITY),
            Shape::Rect { .. } | Shape::Ellipse { .. } | Shape::Mosaic { .. } => {
                // Center is the midpoint of draw_start and draw_current
                if let (Some(start), Some(current)) = (self.draw_start, self.draw_current) {
                    Some(Affine2::from_translation((start + current) / 2.0))
                } else {
                    Some(Affine2::IDENTITY)
                }
            }
        }
    }

    /// Get the drawing start position.
    pub fn draw_start(&self) -> Option<Vec2> {
        self.draw_start
    }

    /// Compute the axis-aligned bounding box of an annotation in global logical space.
    pub fn annotation_bounds(ann: &Annotation) -> Rect {
        match &ann.shape {
            Shape::Pen { points } => {
                if points.is_empty() {
                    return Rect::new(0, 0, 0, 0);
                }
                let mut min_x = f32::MAX;
                let mut min_y = f32::MAX;
                let mut max_x = f32::MIN;
                let mut max_y = f32::MIN;
                for p in points {
                    let gp = ann.transform.transform_point2(*p);
                    min_x = min_x.min(gp.x);
                    min_y = min_y.min(gp.y);
                    max_x = max_x.max(gp.x);
                    max_y = max_y.max(gp.y);
                }
                let hw = ann.stroke_width / 2.0;
                Rect::new(
                    (min_x - hw) as i32,
                    (min_y - hw) as i32,
                    (max_x - min_x + ann.stroke_width) as i32,
                    (max_y - min_y + ann.stroke_width) as i32,
                )
            }
            Shape::Line { start, end } => {
                let gs = ann.transform.transform_point2(*start);
                let ge = ann.transform.transform_point2(*end);
                let min_x = gs.x.min(ge.x);
                let min_y = gs.y.min(ge.y);
                let max_x = gs.x.max(ge.x);
                let max_y = gs.y.max(ge.y);
                let hw = ann.stroke_width / 2.0;
                Rect::new(
                    (min_x - hw) as i32,
                    (min_y - hw) as i32,
                    (max_x - min_x + ann.stroke_width) as i32,
                    (max_y - min_y + ann.stroke_width) as i32,
                )
            }
            Shape::Rect { half_extents } | Shape::Mosaic { half_extents } => {
                let he = *half_extents;
                // Local rect: (-he.x, -he.y) to (he.x, he.y)
                let corners = [
                    ann.transform.transform_point2(Vec2::new(-he.x, -he.y)),
                    ann.transform.transform_point2(Vec2::new(he.x, -he.y)),
                    ann.transform.transform_point2(Vec2::new(-he.x, he.y)),
                    ann.transform.transform_point2(Vec2::new(he.x, he.y)),
                ];
                let min_x = corners.iter().map(|c| c.x).fold(f32::MAX, f32::min);
                let min_y = corners.iter().map(|c| c.y).fold(f32::MAX, f32::min);
                let max_x = corners.iter().map(|c| c.x).fold(f32::MIN, f32::max);
                let max_y = corners.iter().map(|c| c.y).fold(f32::MIN, f32::max);
                let hw = ann.stroke_width / 2.0;
                Rect::new(
                    (min_x - hw) as i32,
                    (min_y - hw) as i32,
                    (max_x - min_x + ann.stroke_width) as i32,
                    (max_y - min_y + ann.stroke_width) as i32,
                )
            }
            Shape::Ellipse { radii } => {
                let r = *radii;
                // Approximate AABB of rotated ellipse
                let cos = ann.transform.x_axis.x;
                let sin = ann.transform.y_axis.x;
                // The axes of the ellipse after rotation
                let ax = Vec2::new(cos, sin) * r.x;
                let ay = Vec2::new(-sin, cos) * r.y;
                let cx = ann.transform.translation.x;
                let cy = ann.transform.translation.y;
                let hw = (ax.x.abs() + ay.x.abs()) + ann.stroke_width / 2.0;
                let hh = (ax.y.abs() + ay.y.abs()) + ann.stroke_width / 2.0;
                Rect::new(
                    (cx - hw) as i32,
                    (cy - hh) as i32,
                    (hw * 2.0) as i32,
                    (hh * 2.0) as i32,
                )
            }
        }
    }

    /// Compute the oriented bounding box of an annotation in global logical space.
    pub fn oriented_bounds(ann: &Annotation) -> OrientedRect {
        let hw = ann.stroke_width / 2.0;
        let (min, max) = match &ann.shape {
            Shape::Pen { points } => {
                if points.is_empty() {
                    return OrientedRect { corners: [Vec2::ZERO; 4] };
                }
                let mut min_v = Vec2::new(f32::MAX, f32::MAX);
                let mut max_v = Vec2::new(f32::MIN, f32::MIN);
                for p in points {
                    min_v = min_v.min(*p);
                    max_v = max_v.max(*p);
                }
                (min_v - Vec2::splat(hw), max_v + Vec2::splat(hw))
            }
            Shape::Line { start, end } => {
                let min_v = start.min(*end);
                let max_v = start.max(*end);
                (min_v - Vec2::splat(hw), max_v + Vec2::splat(hw))
            }
            Shape::Rect { half_extents } | Shape::Mosaic { half_extents } => {
                let he = *half_extents;
                (Vec2::new(-he.x - hw, -he.y - hw), Vec2::new(he.x + hw, he.y + hw))
            }
            Shape::Ellipse { radii } => {
                let r = *radii;
                (Vec2::new(-r.x - hw, -r.y - hw), Vec2::new(r.x + hw, r.y + hw))
            }
        };
        let tl = ann.transform.transform_point2(Vec2::new(min.x, min.y));
        let tr = ann.transform.transform_point2(Vec2::new(max.x, min.y));
        let br = ann.transform.transform_point2(Vec2::new(max.x, max.y));
        let bl = ann.transform.transform_point2(Vec2::new(min.x, max.y));
        OrientedRect { corners: [tl, tr, br, bl] }
    }

    /// Check if a point (in global logical coords) hits an annotation.
    /// Returns the index of the topmost annotation hit, or None.
    pub fn hit_test(&self, pos: Vec2) -> Option<usize> {
        // Iterate in reverse (topmost first)
        for (idx, ann) in self.annotations.iter().enumerate().rev() {
            let bounds = Self::annotation_bounds(ann);
            let p = crate::geom::Point::new(pos.x as i32, pos.y as i32);
            if bounds.contains(p) {
                return Some(idx);
            }
        }
        None
    }

    /// True if the current drawing tool operates on this shape kind.
    fn matches_tool(shape: &Shape, tool: crate::ui::toolbar::Tool) -> bool {
        use crate::ui::toolbar::Tool;
        matches!(
            (shape, tool),
            (Shape::Pen { .. }, Tool::Pen)
                | (Shape::Line { .. }, Tool::Line)
                | (Shape::Rect { .. }, Tool::Rect)
                | (Shape::Ellipse { .. }, Tool::Ellipse)
                | (Shape::Mosaic { .. }, Tool::Mosaic)
        )
    }

    /// Hit-test only annotations whose shape matches the current tool.
    fn hit_test_matching(&self, pos: Vec2, tool: crate::ui::toolbar::Tool) -> Option<usize> {
        let p = crate::geom::Point::new(pos.x as i32, pos.y as i32);
        for (idx, ann) in self.annotations.iter().enumerate().rev() {
            if !Self::matches_tool(&ann.shape, tool) {
                continue;
            }
            if Self::annotation_bounds(ann).contains(p) {
                return Some(idx);
            }
        }
        None
    }

    /// True if an edit drag (move/scale/rotate of a selected annotation) is in progress.
    pub fn has_edit_drag(&self) -> bool {
        self.edit_drag.is_some()
    }

    /// Compute the edit handle positions for the currently selected annotation.
    /// Returns empty vec if no annotation is selected.
    pub fn edit_handles(&self) -> Vec<EditHandlePos> {
        let idx = match self.selected_id {
            Some(i) if i < self.annotations.len() => i,
            _ => return Vec::new(),
        };
        let ob = Self::oriented_bounds(&self.annotations[idx]);
        let mut handles = Vec::with_capacity(5);

        // 4 corner handles at oriented corners
        handles.push(EditHandlePos { kind: EditHandle::Corner(0), pos: ob.corners[0] });
        handles.push(EditHandlePos { kind: EditHandle::Corner(1), pos: ob.corners[1] });
        handles.push(EditHandlePos { kind: EditHandle::Corner(2), pos: ob.corners[2] });
        handles.push(EditHandlePos { kind: EditHandle::Corner(3), pos: ob.corners[3] });

        // Rotation handle: above top center, offset outward from center
        let top_center = (ob.corners[0] + ob.corners[1]) / 2.0;
        let center = ob.center();
        let outward = top_center - center;
        let rot_pos = if outward.length() > 0.01 {
            top_center + outward.normalize() * 20.0
        } else {
            top_center + Vec2::new(0.0, -20.0)
        };
        handles.push(EditHandlePos { kind: EditHandle::Rotation, pos: rot_pos });

        handles
    }

    /// Hit-test edit handles for the currently selected annotation.
    fn hit_test_edit_handle(&self, pos: Vec2) -> Option<EditHandle> {
        for hp in self.edit_handles() {
            if (hp.pos - pos).length() <= HANDLE_TOLERANCE {
                return Some(hp.kind);
            }
        }
        None
    }

    /// Handle a pointer press event in annotation mode.
    /// `pos` is in global logical coordinates.
    /// `tool` must be a drawing tool (Pen/Line/Rect/Ellipse/Mosaic); Move short-circuits.
    /// When `force_new` is true (Alt or Shift held), hit-testing is skipped and a new
    /// shape is always started.
    /// `selection_rect` is the confirmed selection rect in global logical coords.
    pub fn on_pointer_press(
        &mut self,
        pos: (f64, f64),
        button: u32,
        tool: crate::ui::toolbar::Tool,
        force_new: bool,
        _selection_rect: Option<Rect>,
    ) -> AnnotationAction {
        let p = Vec2::new(pos.0 as f32, pos.1 as f32);

        if button != BTN_LEFT {
            return AnnotationAction::None;
        }

        if matches!(tool, crate::ui::toolbar::Tool::Move) {
            return AnnotationAction::None;
        }

        // Unless the user is forcing a new shape, first try to interact with
        // an existing annotation of the same kind: handle first, then body.
        if !force_new {
            if let Some(idx) = self.selected_id {
                if idx < self.annotations.len()
                    && Self::matches_tool(&self.annotations[idx].shape, tool)
                {
                    if let Some(handle) = self.hit_test_edit_handle(p) {
                        let ann = &self.annotations[idx];
                        let start_transform = ann.transform;
                        let ob = Self::oriented_bounds(ann);
                        let start_center = ob.center();
                        match handle {
                            EditHandle::Corner(corner) => {
                                self.edit_drag = Some(EditDrag::ScalingCorner {
                                    corner,
                                    start_pos: p,
                                    start_transform,
                                    start_center,
                                });
                            }
                            EditHandle::Rotation => {
                                let start_angle = (p - start_center).to_angle();
                                self.edit_drag = Some(EditDrag::Rotating {
                                    start_pos: p,
                                    start_angle,
                                    start_transform,
                                    start_center,
                                });
                            }
                        }
                        return AnnotationAction::Consumed;
                    }
                }
            }

            if let Some(idx) = self.hit_test_matching(p, tool) {
                self.selected_id = Some(idx);
                let start_transform = self.annotations[idx].transform;
                self.edit_drag = Some(EditDrag::Moving {
                    start_pos: p,
                    start_transform,
                });
                return AnnotationAction::Consumed;
            }
        }

        // No hit (or forced new): begin drawing. Any prior selection is
        // cleared so its handles disappear for the duration of the draw.
        self.selected_id = None;
        self.edit_drag = None;

        match tool {
            crate::ui::toolbar::Tool::Pen => {
                self.draw_start = Some(p);
                self.drawing = Some(Shape::Pen { points: vec![p] });
            }
            crate::ui::toolbar::Tool::Line => {
                self.draw_start = Some(p);
                self.drawing = Some(Shape::Line { start: p, end: p });
            }
            crate::ui::toolbar::Tool::Rect => {
                self.draw_start = Some(p);
                self.drawing = Some(Shape::Rect { half_extents: Vec2::ZERO });
            }
            crate::ui::toolbar::Tool::Ellipse => {
                self.draw_start = Some(p);
                self.drawing = Some(Shape::Ellipse { radii: Vec2::ZERO });
            }
            crate::ui::toolbar::Tool::Mosaic => {
                self.draw_start = Some(p);
                self.drawing = Some(Shape::Mosaic { half_extents: Vec2::ZERO });
            }
            crate::ui::toolbar::Tool::Move => unreachable!(),
        }
        AnnotationAction::Consumed
    }

    /// Handle a pointer motion event.
    pub fn on_pointer_motion(&mut self, pos: (f64, f64)) {
        let p = Vec2::new(pos.0 as f32, pos.1 as f32);

        // Update in-progress drawing
        if let Some(ref mut shape) = self.drawing {
            self.draw_current = Some(p);
            match shape {
                Shape::Pen { points } => {
                    points.push(p);
                }
                Shape::Line { end, .. } => {
                    *end = p;
                }
                Shape::Rect { half_extents } | Shape::Mosaic { half_extents } => {
                    if let Some(start) = self.draw_start {
                        let half = (p - start) / 2.0;
                        *half_extents = Vec2::new(half.x.abs(), half.y.abs());
                    }
                }
                Shape::Ellipse { radii } => {
                    if let Some(start) = self.draw_start {
                        let diff = p - start;
                        *radii = Vec2::new(diff.x.abs() / 2.0, diff.y.abs() / 2.0);
                    }
                }
            }
            return;
        }

        // Update edit drag
        if let Some(ref mut drag) = self.edit_drag {
            if let Some(idx) = self.selected_id {
                if idx < self.annotations.len() {
                    match drag {
                        EditDrag::Moving { start_pos, start_transform } => {
                            let delta = p - *start_pos;
                            let translation = Affine2::from_translation(delta);
                            self.annotations[idx].transform = translation * *start_transform;
                        }
                        EditDrag::ScalingCorner { corner: _, start_pos, start_transform, start_center } => {
                            let center = *start_center;
                            let start_delta = *start_pos - center;
                            let current_delta = p - center;
                            if start_delta.length() > 1.0 {
                                let scale = current_delta.length() / start_delta.length();
                                let scale_transform = Affine2::from_scale(Vec2::splat(scale));
                                let to_origin = Affine2::from_translation(-center);
                                let from_origin = Affine2::from_translation(center);
                                self.annotations[idx].transform =
                                    from_origin * scale_transform * to_origin * *start_transform;
                            }
                        }
                        EditDrag::Rotating { start_pos: _, start_angle, start_transform, start_center } => {
                            let center = *start_center;
                            let current_angle = (p - center).to_angle();
                            let delta_angle = current_angle - *start_angle;
                            let rotation = Affine2::from_angle(delta_angle);
                            let to_origin = Affine2::from_translation(-center);
                            let from_origin = Affine2::from_translation(center);
                            self.annotations[idx].transform =
                                from_origin * rotation * to_origin * *start_transform;
                        }
                    }
                }
            }
        }
    }

    /// Handle a pointer release event.
    pub fn on_pointer_release(&mut self, _pos: (f64, f64), button: u32) {
        // Always clear edit drag on any button release
        self.edit_drag = None;

        // Save draw_current before clearing (needed for Rect/Ellipse center calculation)
        let saved_draw_current = self.draw_current;
        self.draw_current = None;

        if button != BTN_LEFT {
            return;
        }

        // Finalize drawing
        if let Some(shape) = self.drawing.take() {
            let start = self.draw_start.take();
            let is_valid = match &shape {
                Shape::Pen { points } => points.len() >= 2,
                Shape::Line { start, end } => (*start - *end).length() > 1.0,
                Shape::Rect { half_extents } | Shape::Mosaic { half_extents } => half_extents.x > 1.0 && half_extents.y > 1.0,
                Shape::Ellipse { radii } => radii.x > 1.0 && radii.y > 1.0,
            };

            if is_valid {
                let current = saved_draw_current;
                let transform = match &shape {
                    Shape::Pen { .. } | Shape::Line { .. } => Affine2::IDENTITY,
                    Shape::Rect { .. } | Shape::Ellipse { .. } | Shape::Mosaic { .. } => {
                        // Center at the midpoint of draw_start → draw_current
                        if let (Some(s), Some(c)) = (start, current) {
                            Affine2::from_translation((s + c) / 2.0)
                        } else {
                            Affine2::IDENTITY
                        }
                    }
                };

                self.annotations.push(Annotation {
                    shape,
                    transform,
                    color: DEFAULT_COLOR,
                    stroke_width: DEFAULT_STROKE_WIDTH,
                });
            }
        }

        // End edit drag
        self.edit_drag = None;
    }

    /// Compute the cursor shape for a given position and tool.
    /// Returns None if the annotation system doesn't want to override the cursor.
    pub fn cursor_for_position(
        &self,
        pos: (f64, f64),
        tool: crate::ui::toolbar::Tool,
        selection_rect: Option<&Rect>,
    ) -> Option<CursorShape> {
        let p = Vec2::new(pos.0 as f32, pos.1 as f32);

        match tool {
            crate::ui::toolbar::Tool::Pen
            | crate::ui::toolbar::Tool::Line
            | crate::ui::toolbar::Tool::Rect
            | crate::ui::toolbar::Tool::Ellipse
            | crate::ui::toolbar::Tool::Mosaic => {
                // Active edit drag → Move cursor regardless of position.
                if self.edit_drag.is_some() {
                    return Some(CursorShape::Move);
                }
                // Hovering a handle of the currently selected (matching-shape)
                // annotation → Move cursor (future: dedicated resize/rotate cursors).
                if let Some(idx) = self.selected_id {
                    if idx < self.annotations.len()
                        && Self::matches_tool(&self.annotations[idx].shape, tool)
                        && self.hit_test_edit_handle(p).is_some()
                    {
                        return Some(CursorShape::Move);
                    }
                }
                // Hovering a same-kind annotation body → Move cursor
                // (click will select + move).
                if self.hit_test_matching(p, tool).is_some() {
                    return Some(CursorShape::Move);
                }
                // Otherwise: crosshair inside the selection, defer outside.
                if let Some(rect) = selection_rect {
                    let gp = crate::geom::Point::new(pos.0 as i32, pos.1 as i32);
                    if rect.contains(gp) {
                        return Some(CursorShape::Crosshair);
                    }
                }
                None // Let the selection state machine decide cursor outside selection
            }
            crate::ui::toolbar::Tool::Move => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::toolbar::Tool;

    /// Draw a Line from `a` to `b`.
    fn draw_line(state: &mut AnnotationState, a: (f64, f64), b: (f64, f64)) {
        state.on_pointer_press(a, BTN_LEFT, Tool::Line, false, None);
        state.on_pointer_motion(b);
        state.on_pointer_release(b, BTN_LEFT);
    }

    /// Draw a Rect from `a` to `b`.
    fn draw_rect(state: &mut AnnotationState, a: (f64, f64), b: (f64, f64)) {
        state.on_pointer_press(a, BTN_LEFT, Tool::Rect, false, None);
        state.on_pointer_motion(b);
        state.on_pointer_release(b, BTN_LEFT);
    }

    #[test]
    fn draw_pen_stroke() {
        let mut state = AnnotationState::new();
        state.on_pointer_press((100.0, 100.0), BTN_LEFT, Tool::Pen, false, None);
        state.on_pointer_motion((120.0, 100.0));
        state.on_pointer_motion((140.0, 110.0));
        state.on_pointer_release((140.0, 110.0), BTN_LEFT);

        assert_eq!(state.annotations.len(), 1);
        assert!(matches!(state.annotations[0].shape, Shape::Pen { .. }));
    }

    #[test]
    fn draw_line_shape() {
        let mut state = AnnotationState::new();
        draw_line(&mut state, (100.0, 100.0), (200.0, 200.0));

        assert_eq!(state.annotations.len(), 1);
        if let Shape::Line { start, end } = state.annotations[0].shape {
            assert!((start.x - 100.0).abs() < 1.0);
            assert!((end.x - 200.0).abs() < 1.0);
        } else {
            panic!("expected Line shape");
        }
    }

    #[test]
    fn draw_rect_shape() {
        let mut state = AnnotationState::new();
        draw_rect(&mut state, (100.0, 100.0), (200.0, 200.0));

        assert_eq!(state.annotations.len(), 1);
        if let Shape::Rect { half_extents } = state.annotations[0].shape {
            assert!((half_extents.x - 50.0).abs() < 1.0);
            assert!((half_extents.y - 50.0).abs() < 1.0);
        } else {
            panic!("expected Rect shape");
        }
    }

    #[test]
    fn draw_ellipse_shape() {
        let mut state = AnnotationState::new();
        state.on_pointer_press((100.0, 100.0), BTN_LEFT, Tool::Ellipse, false, None);
        state.on_pointer_motion((200.0, 200.0));
        state.on_pointer_release((200.0, 200.0), BTN_LEFT);

        assert_eq!(state.annotations.len(), 1);
        if let Shape::Ellipse { radii } = state.annotations[0].shape {
            assert!((radii.x - 50.0).abs() < 1.0);
            assert!((radii.y - 50.0).abs() < 1.0);
        } else {
            panic!("expected Ellipse shape");
        }
    }

    #[test]
    fn too_small_line_is_discarded() {
        let mut state = AnnotationState::new();
        draw_line(&mut state, (100.0, 100.0), (100.5, 100.5));
        assert_eq!(state.annotations.len(), 0);
    }

    #[test]
    fn clear_removes_all() {
        let mut state = AnnotationState::new();
        draw_line(&mut state, (100.0, 100.0), (200.0, 200.0));
        assert_eq!(state.annotations.len(), 1);

        state.clear();
        assert_eq!(state.annotations.len(), 0);
        assert!(state.selected_id.is_none());
    }

    #[test]
    fn delete_selected_annotation() {
        let mut state = AnnotationState::new();
        draw_line(&mut state, (100.0, 100.0), (200.0, 200.0));
        draw_line(&mut state, (300.0, 100.0), (400.0, 200.0));
        assert_eq!(state.annotations.len(), 2);

        // Select first annotation
        state.selected_id = Some(0);
        state.on_delete();
        assert_eq!(state.annotations.len(), 1);
        assert!(state.selected_id.is_none());
    }

    #[test]
    fn move_tool_ignores_annotations() {
        let mut state = AnnotationState::new();
        draw_line(&mut state, (100.0, 100.0), (200.0, 200.0));

        let result = state.on_pointer_press((150.0, 150.0), BTN_LEFT, Tool::Move, false, None);
        assert_eq!(result, AnnotationAction::None);
    }

    #[test]
    fn line_tool_selects_existing_line() {
        let mut state = AnnotationState::new();
        draw_line(&mut state, (100.0, 100.0), (200.0, 200.0));

        // Click on the line's bounds in Line mode → selects instead of starting a new draw.
        let result = state.on_pointer_press((150.0, 150.0), BTN_LEFT, Tool::Line, false, None);
        assert_eq!(result, AnnotationAction::Consumed);
        assert_eq!(state.selected_id, Some(0));
        assert!(state.drawing.is_none(), "should not start drawing over existing line");
    }

    #[test]
    fn rect_tool_ignores_non_matching_shape() {
        let mut state = AnnotationState::new();
        // An existing Line — from Rect tool's perspective, this is empty space.
        draw_line(&mut state, (100.0, 100.0), (200.0, 200.0));

        // Click on the line's bounds in Rect mode → starts drawing a new rect,
        // does not select the line.
        state.on_pointer_press((150.0, 150.0), BTN_LEFT, Tool::Rect, false, None);
        assert!(state.selected_id.is_none());
        assert!(matches!(state.drawing, Some(Shape::Rect { .. })));
    }

    #[test]
    fn force_new_bypasses_hit_test() {
        let mut state = AnnotationState::new();
        draw_rect(&mut state, (100.0, 100.0), (200.0, 200.0));
        assert_eq!(state.annotations.len(), 1);

        // force_new=true → click inside the existing rect must begin a new draw,
        // not select the existing rect.
        state.on_pointer_press((150.0, 150.0), BTN_LEFT, Tool::Rect, true, None);
        assert!(state.selected_id.is_none());
        assert!(matches!(state.drawing, Some(Shape::Rect { .. })));
    }

    #[test]
    fn force_new_bypasses_handle_of_selected() {
        let mut state = AnnotationState::new();
        draw_rect(&mut state, (100.0, 100.0), (200.0, 200.0));
        // Pre-select the rect so its handles are active.
        state.selected_id = Some(0);

        let handles = state.edit_handles();
        let corner_pos = handles
            .iter()
            .find(|h| matches!(h.kind, EditHandle::Corner(0)))
            .unwrap()
            .pos;

        // force_new=true over a handle → still starts drawing, not scaling.
        state.on_pointer_press(
            (corner_pos.x as f64, corner_pos.y as f64),
            BTN_LEFT,
            Tool::Rect,
            true,
            None,
        );
        assert!(state.edit_drag.is_none());
        assert!(matches!(state.drawing, Some(Shape::Rect { .. })));
    }

    #[test]
    fn cursor_drawing_inside_selection() {
        let state = AnnotationState::new();
        let sel = Rect::new(0, 0, 500, 500);
        let cursor = state.cursor_for_position((250.0, 250.0), Tool::Pen, Some(&sel));
        assert_eq!(cursor, Some(CursorShape::Crosshair));
    }

    #[test]
    fn cursor_drawing_outside_selection() {
        let state = AnnotationState::new();
        let sel = Rect::new(0, 0, 500, 500);
        let cursor = state.cursor_for_position((600.0, 600.0), Tool::Pen, Some(&sel));
        assert_eq!(cursor, None); // Let selection decide
    }

    #[test]
    fn cursor_over_matching_annotation() {
        let mut state = AnnotationState::new();
        draw_line(&mut state, (100.0, 100.0), (200.0, 200.0));

        // In Line tool, over the existing line → Move cursor (click will select).
        let cursor = state.cursor_for_position((150.0, 150.0), Tool::Line, None);
        assert_eq!(cursor, Some(CursorShape::Move));
    }

    #[test]
    fn cursor_over_non_matching_annotation_defers() {
        let mut state = AnnotationState::new();
        draw_line(&mut state, (100.0, 100.0), (200.0, 200.0));

        // In Rect tool, over a Line → the Line is invisible to hit-testing,
        // so the cursor falls through to the default (crosshair inside selection,
        // None outside).
        let sel = Rect::new(0, 0, 500, 500);
        let cursor = state.cursor_for_position((150.0, 150.0), Tool::Rect, Some(&sel));
        assert_eq!(cursor, Some(CursorShape::Crosshair));
    }

    #[test]
    fn move_annotation_via_drawing_tool() {
        let mut state = AnnotationState::new();
        draw_line(&mut state, (100.0, 100.0), (200.0, 200.0));

        // Select via Line tool and start moving.
        state.on_pointer_press((150.0, 150.0), BTN_LEFT, Tool::Line, false, None);
        let orig_translation = state.annotations[0].transform.translation;

        state.on_pointer_motion((160.0, 160.0));
        let new_translation = state.annotations[0].transform.translation;
        // Translation should have moved by (10, 10)
        assert!((new_translation.x - orig_translation.x - 10.0).abs() < 0.1);
        assert!((new_translation.y - orig_translation.y - 10.0).abs() < 0.1);

        state.on_pointer_release((160.0, 160.0), BTN_LEFT);
        // Translation persists after release
        assert!((state.annotations[0].transform.translation.x - orig_translation.x - 10.0).abs() < 0.1);
    }

    #[test]
    fn edit_handles_exist_for_selected() {
        let mut state = AnnotationState::new();
        draw_rect(&mut state, (100.0, 100.0), (200.0, 200.0));

        // No handles before selection
        assert!(state.edit_handles().is_empty());

        // Select the annotation
        state.selected_id = Some(0);
        let handles = state.edit_handles();
        // 4 corners + 1 rotation = 5 handles
        assert_eq!(handles.len(), 5);
    }

    #[test]
    fn selected_handle_usable_in_drawing_tool() {
        let mut state = AnnotationState::new();
        draw_rect(&mut state, (100.0, 100.0), (200.0, 200.0));

        // Select via Rect tool.
        state.on_pointer_press((150.0, 150.0), BTN_LEFT, Tool::Rect, false, None);
        assert_eq!(state.selected_id, Some(0));
        state.on_pointer_release((150.0, 150.0), BTN_LEFT);

        // Click on a corner handle in the same tool → starts scaling, not drawing.
        let handles = state.edit_handles();
        let corner_pos = handles
            .iter()
            .find(|h| matches!(h.kind, EditHandle::Corner(0)))
            .unwrap()
            .pos;
        state.on_pointer_press(
            (corner_pos.x as f64, corner_pos.y as f64),
            BTN_LEFT,
            Tool::Rect,
            false,
            None,
        );
        assert!(matches!(state.edit_drag, Some(EditDrag::ScalingCorner { .. })));
        assert!(state.drawing.is_none());
    }

    #[test]
    fn selected_rotation_handle_starts_rotation() {
        let mut state = AnnotationState::new();
        draw_rect(&mut state, (100.0, 100.0), (200.0, 200.0));

        // Select via Rect tool.
        state.on_pointer_press((150.0, 150.0), BTN_LEFT, Tool::Rect, false, None);
        state.on_pointer_release((150.0, 150.0), BTN_LEFT);

        // Click on the rotation handle.
        let handles = state.edit_handles();
        let rot_pos = handles
            .iter()
            .find(|h| matches!(h.kind, EditHandle::Rotation))
            .unwrap()
            .pos;
        state.on_pointer_press(
            (rot_pos.x as f64, rot_pos.y as f64),
            BTN_LEFT,
            Tool::Rect,
            false,
            None,
        );
        assert!(matches!(state.edit_drag, Some(EditDrag::Rotating { .. })));
    }

    #[test]
    fn draw_mosaic() {
        let mut state = AnnotationState::new();
        state.on_pointer_press((100.0, 100.0), BTN_LEFT, Tool::Mosaic, false, None);
        state.on_pointer_motion((200.0, 200.0));
        state.on_pointer_release((200.0, 200.0), BTN_LEFT);

        assert_eq!(state.annotations.len(), 1);
        if let Shape::Mosaic { half_extents } = state.annotations[0].shape {
            assert!((half_extents.x - 50.0).abs() < 1.0);
            assert!((half_extents.y - 50.0).abs() < 1.0);
        } else {
            panic!("expected Mosaic shape");
        }
    }

    #[test]
    fn rect_tool_does_not_select_mosaic() {
        let mut state = AnnotationState::new();
        // Mosaic uses the same geometry as Rect, but is a distinct shape kind.
        state.on_pointer_press((100.0, 100.0), BTN_LEFT, Tool::Mosaic, false, None);
        state.on_pointer_motion((200.0, 200.0));
        state.on_pointer_release((200.0, 200.0), BTN_LEFT);
        assert_eq!(state.annotations.len(), 1);

        // In Rect mode, clicking on the Mosaic should begin a new Rect — the
        // Mosaic is not selectable by the Rect tool.
        state.on_pointer_press((150.0, 150.0), BTN_LEFT, Tool::Rect, false, None);
        assert!(state.selected_id.is_none());
        assert!(matches!(state.drawing, Some(Shape::Rect { .. })));
    }

    #[test]
    fn too_small_mosaic_is_discarded() {
        let mut state = AnnotationState::new();
        state.on_pointer_press((100.0, 100.0), BTN_LEFT, Tool::Mosaic, false, None);
        state.on_pointer_motion((100.5, 100.5));
        state.on_pointer_release((100.5, 100.5), BTN_LEFT);

        assert_eq!(state.annotations.len(), 0);
    }
}
