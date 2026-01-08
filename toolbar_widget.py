from PyQt5.QtWidgets import QWidget, QHBoxLayout, QToolButton
from PyQt5.QtCore import Qt, QSize, QRect
from PyQt5.QtGui import QIcon, QColor, QPalette

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
        
        # Buttons
        # Todo: Replace with actual icons and connect actions
        # For now using standard icons or simple text for visualization
        
        # 1. Cancel / Close
        self.btn_close = self._create_button("application-exit", "Close")
        layout.addWidget(self.btn_close)

        # 2. Save
        self.btn_save = self._create_button("document-save", "Save")
        layout.addWidget(self.btn_save)

        # 3. Confirm / Copy (Enter)
        self.btn_confirm = self._create_button("edit-copy", "Copy")
        layout.addWidget(self.btn_confirm)

        # Adjust width based on content
        self.adjustSize()
        
    def _set_style(self):
        # White background, Black border (handled via stylesheet or custom paint if needed)
        # Using stylesheet for simplicity
        self.setStyleSheet("""
            QWidget {
                background-color: white;
                border: 1px solid black;
            }
            QToolButton {
                border: none;
                background: transparent;
                border-right: 1px solid #ccc; /* Optional separator */
                border-radius: 0px;
            }
            QToolButton:last-child {
                border-right: none;
            }
            QToolButton:hover {
                background-color: #eee;
            }
        """)
        
    def _create_button(self, icon_name, tooltip):
        btn = QToolButton()
        btn.setFixedSize(32, 32)
        
        icon = QIcon.fromTheme(icon_name)
        if icon.isNull():
             # Fallback if theme icon missing
             btn.setText(tooltip[0])
        else:
            btn.setIcon(icon)
            
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
