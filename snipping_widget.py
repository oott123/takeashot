from PyQt5.QtWidgets import QWidget, QApplication
from PyQt5.QtCore import Qt, QRect, QSize, pyqtSignal, QTimer, QPoint
from PyQt5.QtGui import QPainter, QColor, QBrush, QPen, QRegion

class SnippingWidget(QWidget):
    closed = pyqtSignal()
    
    def __init__(self, controller, pixmap, x, y, width, height):
        super().__init__()
        self.controller = controller
        self.setWindowState(Qt.WindowFullScreen)
        self.setWindowFlags(Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool | Qt.X11BypassWindowManagerHint)
        self.setAttribute(Qt.WA_DeleteOnClose)
        self.setGeometry(x, y, width, height)
        self.full_pixmap = pixmap
        self.screen_geometry = QRect(x, y, width, height)
        
        self.setMouseTracking(True)
        self.setCursor(Qt.CrossCursor)
        self.show()

    def paintEvent(self, event):
        painter = QPainter(self)
        
        # 0. Fill background black (in case of geometry mismatch)
        painter.fillRect(self.rect(), Qt.black)
        
        # 1. Draw the captured screen
        painter.drawPixmap(0, 0, self.full_pixmap)
        
        # 2. Draw overlay
        # We want everything DARK except the selection INTERSECTED with this screen
        overlay_color = QColor(0, 0, 0, 100) # Semi-transparent black
        
        # Check for pending (snap) selection first
        pending_sel = self.controller.get_pending_selection_rect()
        if not pending_sel.isNull():
            # Draw pending selection (snap preview)
            offset = -self.screen_geometry.topLeft()
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
            offset = -self.screen_geometry.topLeft()
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
            # Only draw if it intersects our screen (window handles clipping mostly, but good to check)
            if local_handle.intersects(self.rect()):
                painter.drawRect(local_handle)

    def handle_cancel_or_exit(self):
        """Handle cancel or exit operation for both right-click and Esc key."""
        # If there's a pending selection, exit directly
        if not self.controller.get_pending_selection_rect().isNull():
            QTimer.singleShot(0, self.close)
        # If there's a real selection, cancel it; otherwise exit
        elif not self.controller.cancel_selection():
            QTimer.singleShot(0, self.close)

    def mousePressEvent(self, event):
        if event.button() == Qt.LeftButton:
            self.controller.on_mouse_press(event.globalPos())
        elif event.button() == Qt.RightButton:
            self.handle_cancel_or_exit()

    def mouseMoveEvent(self, event):
        # Forward move to controller to update active handle / selection
        # But for cursor update (hover), we can query controller
        
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

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.LeftButton:
            self.controller.on_mouse_release()

    def keyPressEvent(self, event):
        if event.key() == Qt.Key_Escape:
            self.handle_cancel_or_exit()
        elif event.key() == Qt.Key_Return or event.key() == Qt.Key_Enter:
            self.controller.capture_selection()
            QTimer.singleShot(0, self.close)

    def mouseDoubleClickEvent(self, event):
        if event.button() == Qt.LeftButton:
            # Only handle double click if there's a real selection
            if not self.controller.selection_rect.isNull():
                # Check if double click is inside the selection
                if self.controller.selection_rect.contains(event.globalPos()):
                    self.controller.capture_selection()
                    QTimer.singleShot(0, self.close)

    def closeEvent(self, event):
        self.closed.emit()
        super().closeEvent(event)
