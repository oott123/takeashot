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
        
        global_sel = self.controller.selection_rect
        if global_sel.isNull():
            painter.fillRect(self.rect(), overlay_color)
        else:
            # We need to draw the global selection in local coordinates.
            # Local (0,0) is self.screen_geometry.topLeft() in global space.
            # So, local_rect = global_rect.translated(-self.screen_geometry.topLeft())
            
            # However, we want to draw the *entire* overlay logic based on the local view of the global state.
            
            # Let's define the local selection rect
            offset = -self.screen_geometry.topLeft()
            local_sel = global_sel.translated(offset)
            
            # Draw overlay around the selection
            # An easy way is to use QRegion subtraction, but rects are faster.
            # We effectively want to fill self.rect() MINUS local_sel
            
            region_all = QRegion(self.rect())
            region_sel = QRegion(local_sel)
            region_overlay = region_all.subtracted(region_sel)
            
            for rect in region_overlay.rects():
                painter.fillRect(rect, overlay_color)
            
            # Draw selection border (of the full global rect, clipped by window)
            pen = QPen(QColor(0, 120, 215), 1)
            painter.setPen(pen)
            painter.setBrush(Qt.NoBrush)
            painter.drawRect(local_sel)
            
            # Draw resize handles
            self.draw_handles(painter, offset)

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

    def mousePressEvent(self, event):
        if event.button() == Qt.LeftButton:
            self.controller.on_mouse_press(event.globalPos())
        elif event.button() == Qt.RightButton:
            QTimer.singleShot(0, self.close)

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
            QTimer.singleShot(0, self.close)
        elif event.key() == Qt.Key_Return or event.key() == Qt.Key_Enter:
            self.controller.capture_selection()
            QTimer.singleShot(0, self.close)

    def closeEvent(self, event):
        self.closed.emit()
        super().closeEvent(event)
