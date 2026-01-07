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
            
        # Also ensure it doesn't go off screen right (though aligning right usually prevents this unless selection is tiny and near left edge)
        # Actually, if selection is small, right aligning might put it to the left of selection right edge, which is fine.
        # But if selection is smaller than toolbar?
        # Requirement: "Right align".
        
        # Let's calculate candidate positions
        
        # Option 1: Bottom (Outside)
        y_bottom = selection_rect.bottom() + 1 # 1px gap
        candidate_bottom = QRect(x, y_bottom, w, h)
        
        # Option 2: Top (Outside)
        y_top = selection_rect.top() - h - 1
        candidate_top = QRect(x, y_top, w, h)
        
        # Option 3: Inside Bottom
        y_inside = selection_rect.bottom() - h
        if y_inside < selection_rect.top(): # If selection is too short, just align to top of selection?
             y_inside = selection_rect.top()
        candidate_inside = QRect(x, y_inside, w, h)
        
        # Check constraints
        
        # Can it fit below?
        if candidate_bottom.bottom() <= parent_rect.bottom():
             self.move(candidate_bottom.topLeft())
             return
             
        # Can it fit above?
        if candidate_top.top() >= parent_rect.top():
            self.move(candidate_top.topLeft())
            return
            
        # Must go inside
        # Note: If selection is tiny, "Inside" might overlap significantly, but per reqs -> Inside Bottom Right
        self.move(candidate_inside.topLeft())
