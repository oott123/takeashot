from PyQt5.QtWidgets import QWidget, QApplication, QVBoxLayout
from PyQt5.QtCore import Qt, QRect, QSize, pyqtSignal, QTimer, QPoint
from PyQt5.QtGui import QPainter, QColor, QBrush, QPen, QRegion
from toolbar_widget import Toolbar

class SnippingWidget(QWidget):
    """
    The widget that handles the actual painting of the screenshot, overlay, and selection.
    It fills the entire SnippingWindow.
    """
    def __init__(self, controller, pixmap):
        super().__init__()
        self.controller = controller
        self.full_pixmap = pixmap
        
        # We need mouse tracking to show handles/cursor
        self.setMouseTracking(True)
        self.setCursor(Qt.CrossCursor)

    def paintEvent(self, event):
        painter = QPainter(self)
        
        # 0. Fill background black (in case of geometry mismatch)
        painter.fillRect(self.rect(), Qt.black)
        
        # 1. Draw the captured screen
        painter.drawPixmap(0, 0, self.full_pixmap)
        
        # 2. Draw overlay
        # We want everything DARK except the selection INTERSECTED with this screen
        overlay_color = QColor(0, 0, 0, 100) # Semi-transparent black
        
        # Screen geometry relative to itself is just rect()
        # But we need to know where we are in global space to map the global selection rect
        # The Window holds the screen geometry info.
        # However, for painting, we can just use the global coordinates transformed to local.
        # This widget fills the window, which is positioned at (x,y)
        
        # We need the global offset of this window. 
        # Since this widget is inside SnippingWindow, and SnippingWindow is at global (x,y).
        # mapToGlobal(QPoint(0,0)) should give us that IF the window is shown.
        # Or we can ask the parent window.
        parent_window = self.window()
        offset = -parent_window.screen_geometry.topLeft()
        
        # Check for pending (snap) selection first
        pending_sel = self.controller.get_pending_selection_rect()
        if not pending_sel.isNull():
            # Draw pending selection (snap preview)
            local_pending = pending_sel.translated(offset)
            
            # Draw overlay around the pending selection
            region_all = QRegion(self.rect())
            region_pending = QRegion(local_pending)
            region_overlay = region_all.subtracted(region_pending)
            
            for rect in region_overlay.rects():
                painter.fillRect(rect, overlay_color)
            
            # Draw pending selection border (same color as real selection)
            pen = QPen(QColor(0, 120, 215), 1)
            painter.setPen(pen)
            painter.setBrush(Qt.NoBrush)
            painter.drawRect(local_pending)
            
            # Don't draw handles for pending selection
        elif not self.controller.selection_rect.isNull():
            # Draw real selection
            local_sel = self.controller.selection_rect.translated(offset)
            
            # Draw overlay around the selection
            region_all = QRegion(self.rect())
            region_sel = QRegion(local_sel)
            region_overlay = region_all.subtracted(region_sel)
            
            for rect in region_overlay.rects():
                painter.fillRect(rect, overlay_color)
            
            # Draw selection border
            pen = QPen(QColor(0, 120, 215), 1)
            painter.setPen(pen)
            painter.setBrush(Qt.NoBrush)
            painter.drawRect(local_sel)
            
            # Draw resize handles
            self.draw_handles(painter, offset)
        else:
            # No selection at all
            painter.fillRect(self.rect(), overlay_color)

    def draw_handles(self, painter, offset):
        handles = self.controller.get_handle_rects()
        painter.setBrush(QBrush(QColor(255, 255, 255)))
        painter.setPen(QPen(QColor(0, 0, 0), 1))
        
        for handle_rect in handles.values():
            # handle_rect is global, map to local
            local_handle = handle_rect.translated(offset)
            # Only draw if it intersects our screen
            if local_handle.intersects(self.rect()):
                painter.drawRect(local_handle)

    def mousePressEvent(self, event):
        if event.button() == Qt.LeftButton:
            self.controller.on_mouse_press(event.globalPos())
        elif event.button() == Qt.RightButton:
            # Propagate up to window to handle close
            self.window().handle_cancel_or_exit()

    def mouseMoveEvent(self, event):
        global_pos = event.globalPos()
        
        if not self.controller.is_selecting:
            handle = self.controller.get_handle_at(global_pos)
            if handle in ['tl', 'br']: self.setCursor(Qt.SizeFDiagCursor)
            elif handle in ['tr', 'bl']: self.setCursor(Qt.SizeBDiagCursor)
            elif handle in ['t', 'b']: self.setCursor(Qt.SizeVerCursor)
            elif handle in ['l', 'r']: self.setCursor(Qt.SizeHorCursor)
            elif handle == 'move': self.setCursor(Qt.SizeAllCursor)
            else: self.setCursor(Qt.CrossCursor)
        
        self.controller.on_mouse_move(global_pos)
        
        # Tell window to maybe update toolbar
        self.window().update_toolbar_position()

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.LeftButton:
            self.controller.on_mouse_release()
            # Toolbar might need to appear now
            self.window().update_toolbar_position()

    def mouseDoubleClickEvent(self, event):
        if event.button() == Qt.LeftButton:
            if not self.controller.selection_rect.isNull():
                if self.controller.selection_rect.contains(event.globalPos()):
                    self.controller.capture_selection()
                    self.window().close_all()


class SnippingWindow(QWidget):
    """
    The top-level window that contains the SnippingWidget and the Toolbar.
    """
    closed = pyqtSignal()
    
    def __init__(self, controller, pixmap, x, y, width, height):
        super().__init__()
        self.controller = controller
        self.full_pixmap = pixmap
        self.screen_geometry = QRect(x, y, width, height)
        # We keep full_pixmap here just to pass it to the widget, 
        # or we let the widget hold it. The Widget needs it for paint.
        
        # Window Setup
        self.setWindowState(Qt.WindowFullScreen)
        self.setWindowFlags(Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool | Qt.X11BypassWindowManagerHint)
        self.setAttribute(Qt.WA_DeleteOnClose)
        self.setGeometry(x, y, width, height)
        
        # Layout container
        # We use absolute positioning for Toolbar (it floats), 
        # but SnippingWidget should fill the window.
        
        self.snipping_widget = SnippingWidget(controller, pixmap)
        self.snipping_widget.setParent(self)
        self.snipping_widget.resize(width, height)
        self.snipping_widget.move(0, 0)
        
        # Toolbar
        self.toolbar = Toolbar(self)
        self.toolbar.hide() # Hidden by default
        
        # Connect Toolbar Buttons
        self.toolbar.btn_close.clicked.connect(self.handle_cancel_or_exit)
        self.toolbar.btn_confirm.clicked.connect(self.handle_confirm_click)
        # self.toolbar.btn_save.clicked.connect(...)
        
        self.show()

    def resizeEvent(self, event):
        self.snipping_widget.resize(event.size())
        super().resizeEvent(event)

    def handle_confirm_click(self):
        self.controller.capture_selection()
        QTimer.singleShot(0, self.close_all)

    def handle_cancel_or_exit(self):
        """Handle cancel or exit operation for both right-click and Esc key."""
        # If there's a pending selection, exit directly
        if not self.controller.get_pending_selection_rect().isNull():
            self.close_all()
        # If there's a real selection, cancel it; otherwise exit
        elif not self.controller.cancel_selection():
            self.close_all()

    def close_all(self):
        # We need to tell the controller to close ALL snipper windows,
        # otherwise we might just close this one screen's window.
        # The controller listens to 'closed' signal usually?
        # Actually in main.py:
        # snipper.closed.connect(self.on_snipper_closed)
        # on_snipper_closed -> close_all_snippers
        # So closing this window is enough to trigger the chain.
        QTimer.singleShot(0, self.close)

    def keyPressEvent(self, event):
        if event.key() == Qt.Key_Escape:
            self.handle_cancel_or_exit()
        elif event.key() == Qt.Key_Return or event.key() == Qt.Key_Enter:
            self.controller.capture_selection()
            self.close_all()

    def update_toolbar_position(self):
        # Update Toolbar Position
        # Use controller to determine the unique snipper that should show the toolbar
        if not self.controller.selection_rect.isNull() and self == self.controller.get_active_toolbar_snipper():
             global_sel = self.controller.selection_rect
             # Convert global selection rect to local coordinates
             offset = -self.screen_geometry.topLeft()
             local_sel = global_sel.translated(offset)
             self.toolbar.update_position(local_sel, self.rect())
             self.toolbar.raise_() # Ensure toolbar is on top of snipping widget
        else:
             self.toolbar.hide()

    def closeEvent(self, event):
        self.closed.emit()
        super().closeEvent(event)
