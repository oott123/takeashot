"""Freehand drawing annotation tool."""

from .base_tool import BaseTool
from PyQt5.QtCore import QPoint, QRect
from PyQt5.QtGui import QPainter


class FreehandTool(BaseTool):
    """Freehand drawing tool for annotations."""

    def __init__(self):
        super().__init__()
        self.points = []

    def on_mouse_press(self, pos: QPoint, selection_rect: QRect) -> None:
        """Handle mouse press - start drawing."""
        if self.is_in_selection(pos, selection_rect):
            self.points.append(pos)

    def on_mouse_move(self, pos: QPoint, selection_rect: QRect) -> None:
        """Handle mouse move - continue drawing."""
        if self.is_in_selection(pos, selection_rect) and self.points:
            self.points.append(pos)

    def on_mouse_release(self, pos: QPoint, selection_rect: QRect) -> None:
        """Handle mouse release - finish drawing."""
        if self.is_in_selection(pos, selection_rect) and self.points:
            self.points.append(pos)

    def paint(self, painter: QPainter, offset: QPoint, selection_rect: QRect) -> None:
        """Render the freehand drawing."""
        if not self.points:
            return

        painter.setPen(self.get_pen())

        # Draw lines between consecutive points
        for i in range(len(self.points) - 1):
            p1 = self.points[i] - offset
            p2 = self.points[i + 1] - offset
            painter.drawLine(p1, p2)

    def get_annotation_rect(self) -> QRect:
        """Get the bounding rect of this annotation."""
        if not self.points:
            return QRect()

        min_x = min(p.x() for p in self.points)
        max_x = max(p.x() for p in self.points)
        min_y = min(p.y() for p in self.points)
        max_y = max(p.y() for p in self.points)

        return QRect(min_x, min_y, max_x - min_x, max_y - min_y)

    def is_complete(self) -> bool:
        """Check if annotation is finished."""
        return len(self.points) > 0