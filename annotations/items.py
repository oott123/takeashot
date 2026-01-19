import math
from PyQt6.QtCore import Qt, QPointF, QRectF, QSizeF
from PyQt6.QtGui import QPainter, QPen, QBrush, QColor, QPainterPath, QTransform, QPainterPathStroker

class AnnotationItem:
    def __init__(self, start_pos, color=Qt.GlobalColor.red, width=2):
        self.pos = start_pos  # Global position (usually center or top-left depending on item)
        self.rotation = 0.0   # Degrees
        self.color = QColor(color)
        self.width = width
        self.selected = False
        # For simplicity in this non-QGraphicsView system, let's treat `self.rect` as the Unrotated Bounding Box in Global Coords
        # AND `self.rotation` applies around `self.rect.center()`.
        self.rect = QRectF(start_pos, start_pos)

    def update_geometry(self, p1, p2):
        """Called during creation dragging"""
        self.rect = QRectF(p1, p2).normalized()

    def draw(self, painter):
        raise NotImplementedError

    def get_local_point(self, global_point):
        center = self.rect.center()
        t = QTransform()
        t.translate(center.x(), center.y())
        t.rotate(self.rotation)
        t.translate(-center.x(), -center.y())
        inv_t, ok = t.inverted()
        return inv_t.map(global_point)

    def contains(self, point):
        """Hit test taking rotation into account"""
        local_point = self.get_local_point(point)
        return self.rect.contains(local_point)

    def get_transform(self):
        center = self.rect.center()
        t = QTransform()
        t.translate(center.x(), center.y())
        t.rotate(self.rotation)
        t.translate(-center.x(), -center.y())
        return t

    def move(self, delta):
        self.rect.translate(delta)

    def rotate(self, angle_delta):
        self.rotation += angle_delta

    def draw_selection_ui(self, painter):
        # Drawn in local coordinates (self.rect)
        r = self.rect
        h = 6 # Handle size
        
        pen = QPen(Qt.GlobalColor.blue, 1)
        brush = QBrush(Qt.GlobalColor.white)
        painter.setPen(pen)
        painter.setBrush(brush)
        
        # Corners
        handles = [
            r.topLeft(), r.topRight(), r.bottomLeft(), r.bottomRight()
        ]
        
        for pt in handles:
            painter.drawRect(QRectF(pt.x() - h/2, pt.y() - h/2, h, h))
            
        # Rotate handle (above top center)
        top_center = QPointF(r.center().x(), r.top())
        rot_handle = QPointF(top_center.x(), top_center.y() - 15)
        
        painter.drawLine(top_center, rot_handle)
        painter.drawEllipse(rot_handle, h/2, h/2)

    def get_handle_at(self, global_pos):
        local_pos = self.get_local_point(global_pos)
        r = self.rect
        h = 8 # Hit threshold
        
        # Rotate handle
        top_center = QPointF(r.center().x(), r.top())
        rot_handle = QPointF(top_center.x(), top_center.y() - 15)
        if (local_pos - rot_handle).manhattanLength() < h:
            return 'rotate'
        
        # Corners
        if (local_pos - r.topLeft()).manhattanLength() < h: return 'tl'
        if (local_pos - r.topRight()).manhattanLength() < h: return 'tr'
        if (local_pos - r.bottomLeft()).manhattanLength() < h: return 'bl'
        if (local_pos - r.bottomRight()).manhattanLength() < h: return 'br'
        
        return None

    def resize(self, handle, global_pos):
        """
        Resize logic in local space.
        global_pos is converted to local space.
        """
        local_pos = self.get_local_point(global_pos)
        r = self.rect
        
        new_r = QRectF(r)
        
        if handle == 'tl': new_r.setTopLeft(local_pos)
        elif handle == 'tr': new_r.setTopRight(local_pos)
        elif handle == 'bl': new_r.setBottomLeft(local_pos)
        elif handle == 'br': new_r.setBottomRight(local_pos)
        
        self.rect = new_r.normalized()

class RectItem(AnnotationItem):
    def draw(self, painter):
        painter.save()
        t = self.get_transform()
        painter.setTransform(t, combine=True)
        
        pen = QPen(self.color, self.width)
        # pen.setJoinStyle(Qt.PenJoinStyle.MiterJoin)
        painter.setPen(pen)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawRect(self.rect)
        
        if self.selected:
            self.draw_selection_ui(painter)
            
        painter.restore()

    def draw_selection_ui(self, painter):
        # Draw dashed outline logic or handles
        # Handles are drawn in global coords usually, or local?
        # Let's draw in local (already transformed)
        pass # Handle drawing logic deferred to Manager or simple implementation

class EllipseItem(RectItem):
    def draw(self, painter):
        painter.save()
        t = self.get_transform()
        painter.setTransform(t, combine=True)
        
        pen = QPen(self.color, self.width)
        painter.setPen(pen)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawEllipse(self.rect)
        
        painter.restore()

class LineItem(AnnotationItem):
    def __init__(self, start_pos, color=Qt.GlobalColor.red, width=2):
        super().__init__(start_pos, color, width)
        self.p1 = start_pos
        self.p2 = start_pos
    
    def update_geometry(self, p1, p2):
        self.p1 = p1
        self.p2 = p2
        self.rect = QRectF(p1, p2).normalized()

    def draw(self, painter):
        painter.save()
        center = self.rect.center()
        t = QTransform()
        t.translate(center.x(), center.y())
        t.rotate(self.rotation)
        t.translate(-center.x(), -center.y())
        painter.setTransform(t, combine=True)
        
        pen = QPen(self.color, self.width)
        pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        painter.setPen(pen)
        painter.drawLine(self.p1, self.p2)
        
        if self.selected:
             # Draw simplistic selection UI for line (endpoints)
             # Note: Handles should be unrotated space
             pen = QPen(Qt.GlobalColor.blue, 1)
             brush = QBrush(Qt.GlobalColor.white)
             painter.setPen(pen)
             painter.setBrush(brush)
             h = 8
             painter.drawEllipse(self.p1, h/2, h/2)
             painter.drawEllipse(self.p2, h/2, h/2)
        
        painter.restore()

    def move(self, delta):
        super().move(delta)
        # Apply delta to unrotated p1/p2?
        # But super().move translates rect.
        # If we rotate, then move?
        # Our model: Rotation is around RECT center.
        # If we move RECT, center moves.
        # p1/p2 are "local" to the rect? No they are global coordinates.
        # If we move rect, we MUST move p1/p2 too.
        self.p1 += delta
        self.p2 += delta

    def get_handle_at(self, global_pos):
        local_pos = self.get_local_point(global_pos)
        h = 10
        if (local_pos - self.p1).manhattanLength() < h: return 'p1'
        if (local_pos - self.p2).manhattanLength() < h: return 'p2'
        return None

    def resize(self, handle, global_pos):
        # Line resize = moving endpoints
        local_pos = self.get_local_point(global_pos)
        if handle == 'p1': self.p1 = local_pos
        elif handle == 'p2': self.p2 = local_pos
        self.rect = QRectF(self.p1, self.p2).normalized()
        
    def contains(self, point):
        # Hit test for line is distance to segment
        local_p = self.get_local_point(point)
        
        threshold = self.width + 5
        if not self.rect.adjusted(-threshold, -threshold, threshold, threshold).contains(local_p):
            return False
        return self.distance_point_to_segment(local_p, self.p1, self.p2) < threshold

    def distance_point_to_segment(self, p, a, b):
        x, y = p.x(), p.y()
        x1, y1 = a.x(), a.y()
        x2, y2 = b.x(), b.y()
        
        dx = x2 - x1
        dy = y2 - y1
        if dx == 0 and dy == 0:
            return math.hypot(x - x1, y - y1)
            
        t = ((x - x1) * dx + (y - y1) * dy) / (dx*dx + dy*dy)
        t = max(0, min(1, t))
        
        proj_x = x1 + t * dx
        proj_y = y1 + t * dy
        
        return math.hypot(x - proj_x, y - proj_y)

class StrokeItem(AnnotationItem):
    """Freehand pencil stroke"""
    def __init__(self, start_pos, color=Qt.GlobalColor.red, width=2):
        super().__init__(start_pos, color, width)
        self.points = [start_pos]
        self.path = QPainterPath()
        self.path.moveTo(start_pos)
    
    def add_point(self, pos):
        self.points.append(pos)
        self.path.lineTo(pos)
        self.rect = self.path.boundingRect() # Update bounds
        
    def update_geometry(self, p1, p2):
        pass

    def draw(self, painter):
        painter.save()
        
        center = self.rect.center()
        t = QTransform()
        t.translate(center.x(), center.y())
        t.rotate(self.rotation)
        t.translate(-center.x(), -center.y())
        painter.setTransform(t, combine=True)

        pen = QPen(self.color, self.width)
        pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        painter.setPen(pen)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawPath(self.path)
        
        if self.selected:
            self.draw_selection_ui(painter)
            
        painter.restore()

    def move(self, delta):
        super().move(delta)
        self.path.translate(delta)
        for i in range(len(self.points)):
             self.points[i] += delta

    def resize(self, handle, global_pos):
        # Scale path to fit new rect
        # 1. Calculate new rect using base logic
        old_rect = QRectF(self.rect)
        super().resize(handle, global_pos) # Updates self.rect
        new_rect = self.rect
        
        if old_rect.width() == 0 or old_rect.height() == 0:
            return

        # 2. Compute scale factor
        sx = new_rect.width() / old_rect.width()
        sy = new_rect.height() / old_rect.height()
        
        # 3. Transform path
        # Transform needs to map OldRect to NewRect (TopLeft based)
        # T = Translate(-OldBounds) * Scale(sx, sy) * Translate(NewBounds)
        t = QTransform()
        t.translate(new_rect.x(), new_rect.y())
        t.scale(sx, sy)
        t.translate(-old_rect.x(), -old_rect.y())
        
        self.path = t.map(self.path)
        
        # Update points too if needed (though points list might become stale if we rely on path mostly)
        self.points = [t.map(p) for p in self.points]

    def contains(self, point):
        # Path hit testing
        local_p = self.get_local_point(point)
        
        threshold = self.width + 5
        if not self.rect.adjusted(-threshold, -threshold, threshold, threshold).contains(local_p):
            return False
            
        stroke_path = QPainterPathStroker()
        stroke_path.setWidth(threshold * 2)
        outline = stroke_path.createStroke(self.path)
        return outline.contains(local_p)

