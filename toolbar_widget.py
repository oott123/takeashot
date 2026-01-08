from PyQt5.QtWidgets import QWidget, QHBoxLayout, QToolButton
from PyQt5.QtCore import Qt, QSize, QRect
from PyQt5.QtGui import QIcon, QColor, QPalette, QPixmap, QPainter, QPen

class Toolbar(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedHeight(32)
        
        # UI Styling
        self.setAutoFillBackground(True)
        self._set_style()
        
        # Layout
        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)
        
        # Buttons with custom black icons
        
        # 1. Cancel / Close (X icon)
        self.btn_close = self._create_button(self._create_close_icon(), "Close")
        layout.addWidget(self.btn_close)

        # 2. Save (Floppy disk icon)
        self.btn_save = self._create_button(self._create_save_icon(), "Save")
        layout.addWidget(self.btn_save)

        # 3. Confirm / Copy (Checkmark icon)
        self.btn_confirm = self._create_button(self._create_copy_icon(), "Copy")
        layout.addWidget(self.btn_confirm)

        # Adjust width based on content
        self.adjustSize()
        
    def _set_style(self):
        # Style for buttons only - background will be drawn in paintEvent
        self.setStyleSheet("""
            QToolButton {
                border: none;
                background: transparent;
                border-radius: 0px;
            }
            QToolButton:hover {
                background-color: #eee;
            }
        """)
    
    def paintEvent(self, event):
        """Custom paint event to ensure white background and black border are always visible"""
        painter = QPainter(self)
        
        # Draw white background
        painter.fillRect(self.rect(), QColor(255, 255, 255))
        
        # Draw black border
        painter.setPen(QPen(Qt.black, 1))
        painter.setBrush(Qt.NoBrush)
        painter.drawRect(self.rect().adjusted(0, 0, -1, -1))
        
        super().paintEvent(event)
    
    def _create_close_icon(self):
        """Create a black X icon"""
        pixmap = QPixmap(24, 24)
        pixmap.fill(Qt.transparent)
        painter = QPainter(pixmap)
        painter.setRenderHint(QPainter.Antialiasing)
        pen = QPen(Qt.black, 2)
        painter.setPen(pen)
        
        # Draw X
        painter.drawLine(6, 6, 18, 18)
        painter.drawLine(18, 6, 6, 18)
        
        painter.end()
        return QIcon(pixmap)
    
    def _create_save_icon(self):
        """Create a black save/floppy disk icon"""
        pixmap = QPixmap(24, 24)
        pixmap.fill(Qt.transparent)
        painter = QPainter(pixmap)
        painter.setRenderHint(QPainter.Antialiasing)
        pen = QPen(Qt.black, 2)
        painter.setPen(pen)
        
        # Draw floppy disk outline
        painter.drawRect(5, 4, 14, 16)
        # Draw top notch
        painter.drawLine(15, 4, 15, 8)
        painter.drawLine(15, 8, 19, 8)
        painter.drawLine(19, 8, 19, 20)
        # Draw bottom save bar
        painter.drawRect(7, 15, 10, 5)
        
        painter.end()
        return QIcon(pixmap)
    
    def _create_copy_icon(self):
        """Create a black checkmark icon"""
        pixmap = QPixmap(24, 24)
        pixmap.fill(Qt.transparent)
        painter = QPainter(pixmap)
        painter.setRenderHint(QPainter.Antialiasing)
        pen = QPen(Qt.black, 2.5)
        painter.setPen(pen)
        
        # Draw checkmark
        painter.drawLine(6, 12, 10, 17)
        painter.drawLine(10, 17, 18, 7)
        
        painter.end()
        return QIcon(pixmap)
        
    def _create_button(self, icon, tooltip):
        btn = QToolButton()
        btn.setFixedSize(32, 32)
        btn.setIcon(icon)
        btn.setIconSize(QSize(24, 24))
        btn.setToolTip(tooltip)
        return btn

    def update_position(self, selection_rect: QRect, parent_rect: QRect):
        """
        Intelligently position the toolbar based on selection.
        Priority:
        1. Outside Bottom Right
        2. Outside Top Right
        3. Inside Bottom Right
        """
        if selection_rect.isNull():
            self.hide()
            return
            
        self.show()
        
        w = self.width()
        h = self.height()
        
        # Target X: Right aligned with selection
        # But ensure it doesn't go off screen left
        x = selection_rect.right() - w
        if x < parent_rect.left():
            x = parent_rect.left()
            
        # Also ensure it doesn't go off screen right
        if x + w > parent_rect.left() + parent_rect.width():
            x = parent_rect.left() + parent_rect.width() - w
        
        # Let's calculate candidate positions
        
        # 1. Prefer Outside Bottom
        y = selection_rect.bottom() + 1
        if y + h <= parent_rect.bottom():
             self.move(x, y)
             return

        # 2. Prefer Outside Top
        y = selection_rect.top() - h - 1
        if y >= parent_rect.top():
             self.move(x, y)
             return

        # 3. Inside Bottom (clamped to screen)
        # We want it at the bottom of the selection, BUT if that is off-screen, we pin it to bottom of screen.
        target_bottom = min(selection_rect.bottom(), parent_rect.bottom())
        y = target_bottom - h
        
        # Ensure it doesn't go off top of screen (if selection is tiny or screen is tiny)
        if y < parent_rect.top():
            y = parent_rect.top()
            
        self.move(x, y)
