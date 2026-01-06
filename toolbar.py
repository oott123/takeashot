"""Annotation toolbar UI component."""

from PyQt5.QtWidgets import QWidget, QHBoxLayout, QPushButton, QToolButton
from PyQt5.QtCore import Qt, pyqtSignal, QSize, QRect


class AnnotationToolbar(QWidget):
    """Toolbar for annotation tools."""

    tool_selected = pyqtSignal(str)  # Emits tool type: 'freehand', 'rectangle', 'line', None
    clear_requested = pyqtSignal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setup_ui()
        self.current_tool = None

    def setup_ui(self):
        """Setup toolbar UI."""
        self.setWindowFlags(Qt.FramelessWindowHint | Qt.Tool | Qt.WindowStaysOnTopHint)
        self.setAttribute(Qt.WA_TranslucentBackground)

        layout = QHBoxLayout()
        layout.setContentsMargins(8, 4, 8, 4)
        layout.setSpacing(4)

        # Freehand tool button
        self.btn_freehand = self.create_tool_button('✏️', 'freehand')
        layout.addWidget(self.btn_freehand)

        # Rectangle tool button
        self.btn_rectangle = self.create_tool_button('⬜', 'rectangle')
        layout.addWidget(self.btn_rectangle)

        # Line tool button
        self.btn_line = self.create_tool_button('📏', 'line')
        layout.addWidget(self.btn_line)

        # Separator
        separator = QWidget()
        separator.setFixedWidth(1)
        separator.setStyleSheet("background-color: rgba(255, 255, 255, 0.3);")
        layout.addWidget(separator)

        # Clear button
        self.btn_clear = QPushButton('🗑️')
        self.btn_clear.setFixedSize(32, 32)
        self.btn_clear.setFlat(True)
        self.btn_clear.setStyleSheet(self.get_button_style())
        self.btn_clear.clicked.connect(self.on_clear_clicked)
        layout.addWidget(self.btn_clear)

        self.setLayout(layout)

    def create_tool_button(self, icon_text: str, tool_type: str) -> QToolButton:
        """Create a tool button."""
        btn = QToolButton()
        btn.setText(icon_text)
        btn.setFixedSize(32, 32)
        btn.setCheckable(True)
        btn.setStyleSheet(self.get_button_style())
        btn.clicked.connect(lambda: self.on_tool_clicked(tool_type))
        return btn

    def get_button_style(self) -> str:
        """Get button stylesheet."""
        return """
            QToolButton, QPushButton {
                background-color: rgba(40, 40, 40, 200);
                color: white;
                border: 1px solid rgba(255, 255, 255, 0.3);
                border-radius: 4px;
                font-size: 16px;
            }
            QToolButton:hover, QPushButton:hover {
                background-color: rgba(60, 60, 60, 200);
            }
            QToolButton:checked {
                background-color: rgba(0, 120, 215, 200);
                border-color: rgba(0, 120, 215, 255);
            }
        """

    def on_tool_clicked(self, tool_type: str):
        """Handle tool button click."""
        # Toggle button states
        self.btn_freehand.setChecked(tool_type == 'freehand')
        self.btn_rectangle.setChecked(tool_type == 'rectangle')
        self.btn_line.setChecked(tool_type == 'line')

        # If clicking the same tool, deselect it
        if self.current_tool == tool_type:
            self.current_tool = None
            self.btn_freehand.setChecked(False)
            self.btn_rectangle.setChecked(False)
            self.btn_line.setChecked(False)
            self.tool_selected.emit(None)
        else:
            self.current_tool = tool_type
            self.tool_selected.emit(tool_type)

    def on_clear_clicked(self):
        """Handle clear button click."""
        self.clear_requested.emit()

    def position_below_selection(self, selection_rect: QRect, screen_geometry: QRect) -> None:
        """Position toolbar below the selection."""
        # Get toolbar size (account for DPI)
        dpr = self.devicePixelRatio()
        toolbar_width = self.width() / dpr
        toolbar_height = self.height() / dpr

        # Left align with selection, below it (using global logical coordinates)
        x = selection_rect.left()
        y = selection_rect.bottom() + 10

        # Keep within screen bounds (logical coordinates)
        x = max(screen_geometry.left(), min(x, screen_geometry.right() - toolbar_width))
        y = max(screen_geometry.top(), min(y, screen_geometry.bottom() - toolbar_height))

        # Move using logical coordinates
        self.move(x, y)

    def show_for_selection(self, selection_rect: QRect, screen_geometry: QRect) -> None:
        """Show toolbar and position it."""
        self.position_below_selection(selection_rect, screen_geometry)
        self.show()

    def hide_toolbar(self) -> None:
        """Hide toolbar."""
        self.hide()