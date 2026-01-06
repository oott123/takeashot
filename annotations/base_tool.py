"""Base class for annotation tools."""

from abc import ABC, abstractmethod
from PyQt5.QtCore import QPoint, QRect
from PyQt5.QtGui import QPainter, QPen, QColor
from PyQt5.QtCore import Qt


class BaseTool(ABC):
    """Abstract base class for annotation tools."""

    def __init__(self):
        # Default styles (fixed as per requirements)
        self.color = QColor(255, 0, 0)  # Red
        self.width = 3  # Line thickness

    @abstractmethod
    def on_mouse_press(self, pos: QPoint, selection_rect: QRect) -> None:
        """Handle mouse press in annotation mode."""
        pass

    @abstractmethod
    def on_mouse_move(self, pos: QPoint, selection_rect: QRect) -> None:
        """Handle mouse move in annotation mode."""
        pass

    @abstractmethod
    def on_mouse_release(self, pos: QPoint, selection_rect: QRect) -> None:
        """Handle mouse release in annotation mode."""
        pass

    @abstractmethod
    def paint(self, painter: QPainter, offset: QPoint, selection_rect: QRect) -> None:
        """Render the annotation."""
        pass

    @abstractmethod
    def get_annotation_rect(self) -> QRect:
        """Get the bounding rect of this annotation."""
        pass

    @abstractmethod
    def is_complete(self) -> bool:
        """Check if annotation is finished."""
        pass

    def get_pen(self) -> QPen:
        """Get configured pen for drawing."""
        return QPen(self.color, self.width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin)

    def is_in_selection(self, pos: QPoint, selection_rect: QRect) -> bool:
        """Check if position is within selection."""
        return selection_rect.contains(pos)