"""Line drawing annotation tool."""

from .base_tool import BaseTool
from PyQt5.QtCore import QPoint, QRect
from PyQt5.QtGui import QPainter


class LineTool(BaseTool):
    """Line drawing tool for annotations."""

    def __init__(self):
        super().__init__()
        self.start_pos = None
        self.current_pos = None

    def on_mouse_press(self, pos: QPoint, selection_rect: QRect) -> None:
        """Handle mouse press - start line."""
        if self.is_in_selection(pos, selection_rect):
            self.start_pos = pos
            self.current_pos = pos

    def on_mouse_move(self, pos: QPoint, selection_rect: QRect) -> None:
        """Handle mouse move - update line."""
        if self.start_pos and self.is_in_selection(pos, selection_rect):
            self.current_pos = pos

    def on_mouse_release(self, pos: QPoint, selection_rect: QRect) -> None:
        """Handle mouse release - finish line."""
        if self.start_pos and self.is_in_selection(pos, selection_rect):
            self.current_pos = pos

    def paint(self, painter: QPainter, offset: QPoint, selection_rect: QRect) -> None:
        """Render the line."""
        if self.start_pos and self.current_pos:
            painter.setPen(self.get_pen())

            local_start = self.start_pos - offset
            local_end = self.current_pos - offset

            painter.drawLine(local_start, local_end)

    def get_annotation_rect(self) -> QRect:
        """Get the bounding rect of this annotation."""
        if self.start_pos and self.current_pos:
            rect = QRect(self.start_pos, self.current_pos).normalized()
            # Add padding for line width
            padding = self.width // 2 + 2
            return rect.adjusted(-padding, -padding, padding, padding)
        return QRect()

    def is_complete(self) -> bool:
        """Check if annotation is finished."""
        return self.start_pos is not None and self.current_pos is not None