import dbus
import tempfile
import sys
import os
from PyQt5.QtWidgets import QWidget, QApplication, QRubberBand
from PyQt5.QtCore import Qt, QRect, QPoint, QSize, pyqtSignal, QBuffer, QIODevice
from PyQt5.QtGui import QPixmap, QPainter, QColor, QImage, QCursor, QBrush, QPen, QGuiApplication

class ScreenshotBackend:
    def capture(self):
        """
        Captures the screen using KWin DBus interface.
        Returns a QPixmap.
        """
        try:
            bus = dbus.SessionBus()
            obj = bus.get_object('org.kde.KWin', '/org/kde/KWin/ScreenShot2')
            interface = dbus.Interface(obj, 'org.kde.KWin.ScreenShot2')

            with tempfile.TemporaryFile() as tf:
                fd = tf.fileno()
                dbus_fd = dbus.types.UnixFd(fd)
                options = {'native-resolution': True} # Try to request native resolution if supported

                # Attempt to capture workspace (all screens) first, fallback to active screen
                # Note: introspection would confirm if CaptureWorkspace exists. 
                # screenshot_kwin used CaptureActiveScreen. 
                # We will try CaptureWorkspace first to support multi-monitor.
                try:
                    print("Attempting CaptureWorkspace...")
                    metadata = interface.CaptureWorkspace(options, dbus_fd)
                except dbus.DBusException:
                    print("CaptureWorkspace failed, falling back to CaptureActiveScreen...")
                    metadata = interface.CaptureActiveScreen(options, dbus_fd)

                tf.seek(0)
                data = tf.read()

                if not data:
                    print("Error: Empty data received")
                    return None

                width = int(metadata.get('width', 0))
                height = int(metadata.get('height', 0))
                stride = int(metadata.get('stride', 0))
                fmt = int(metadata.get('format', 0))

                if width <= 0 or height <= 0:
                    print(f"Invalid dimensions: {width}x{height}")
                    return None
                
                # Assume ARGB32 for KWin (Format 5 or 6 usually)
                # KWin typically returns BGRA. 
                # QImage.Format_ARGB32 expects data in B G R A order (Little Endian).
                # So we can use Format_ARGB32 directly.
                
                # We must keep a copy of data because QImage doesn't own it by default if passed as bytes
                # But QImage(bytes, ...) needs the bytes to stay alive.
                # We can copy the data into a bytearray or let QImage copy it.
                # QImage(data, width, height, stride, format).copy() makes a deep copy.
                
                # Create QImage from raw data
                img = QImage(data, width, height, stride, QImage.Format_ARGB32)
                
                # Make a deep copy so QImage owns the data
                pixmap = QPixmap.fromImage(img.copy())
                
                # Setup HiDPI support: set device pixel ratio
                # We use the primary screen's ratio. On most systems this is correct.
                # For multi-monitor with mixed DPI, it's more complex, but this fixes the 
                # "enlarged" issue in common HiDPI setups.
                screen = QGuiApplication.primaryScreen()
                ratio = screen.devicePixelRatio() if screen else 1.0
                pixmap.setDevicePixelRatio(ratio)
                
                return pixmap

        except Exception as e:
            print(f"Capture failed: {e}")
            return None


class SnippingWidget(QWidget):
    closed = pyqtSignal()
    
    def __init__(self, pixmap, x, y, width, height):
        super().__init__()
        self.setWindowState(Qt.WindowFullScreen)
        self.setWindowFlags(Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool | Qt.X11BypassWindowManagerHint)
        self.setAttribute(Qt.WA_DeleteOnClose)
        self.setGeometry(x, y, width, height)
        self.full_pixmap = pixmap
        self.screen_geometry = QRect(x, y, width, height)
        
        # Selection state
        self.selection_rect = QRect()
        self.is_selecting = False
        self.resize_handle_size = 8
        self.active_handle = None # 'tl', 't', 'tr', 'r', 'br', 'b', 'bl', 'l', 'move'
        
        self.setCursor(Qt.CrossCursor)
        self.show()

    def paintEvent(self, event):
        painter = QPainter(self)
        
        # 0. Fill background black (in case of geometry mismatch)
        painter.fillRect(self.rect(), Qt.black)
        
        # 1. Draw the captured screen
        painter.drawPixmap(0, 0, self.full_pixmap)
        
        # 2. Draw overlay
        # We want everything DARK except the selection
        overlay_color = QColor(0, 0, 0, 100) # Semi-transparent black
        
        if self.selection_rect.isNull():
            painter.fillRect(self.rect(), overlay_color)
        else:
            # Draw 4 rectangles around the selection
            # Top
            r_top = QRect(0, 0, self.width(), self.selection_rect.top())
            # Bottom
            r_bottom = QRect(0, self.selection_rect.bottom() + 1, self.width(), self.height() - self.selection_rect.bottom() - 1)
            # Left
            r_left = QRect(0, self.selection_rect.top(), self.selection_rect.left(), self.selection_rect.height())
            # Right
            r_right = QRect(self.selection_rect.right() + 1, self.selection_rect.top(), self.width() - self.selection_rect.right() - 1, self.selection_rect.height())
            
            painter.fillRect(r_top, overlay_color)
            painter.fillRect(r_bottom, overlay_color)
            painter.fillRect(r_left, overlay_color)
            painter.fillRect(r_right, overlay_color)
            
            # Draw selection border
            pen = QPen(QColor(0, 120, 215), 1)
            painter.setPen(pen)
            painter.setBrush(Qt.NoBrush)
            painter.drawRect(self.selection_rect)
            
            # Draw resize handles
            self.draw_handles(painter)

    def draw_handles(self, painter):
        if self.selection_rect.isNull():
            return
            
        handles = self.get_handle_rects()
        painter.setBrush(QBrush(QColor(255, 255, 255)))
        painter.setPen(QPen(QColor(0, 0, 0), 1))
        
        for handle_rect in handles.values():
            painter.drawRect(handle_rect)

    def get_handle_rects(self):
        r = self.selection_rect
        s = self.resize_handle_size
        hs = s // 2
        
        return {
            'tl': QRect(r.left() - hs, r.top() - hs, s, s),
            't':  QRect(r.center().x() - hs, r.top() - hs, s, s),
            'tr': QRect(r.right() - hs, r.top() - hs, s, s),
            'r':  QRect(r.right() - hs, r.center().y() - hs, s, s),
            'br': QRect(r.right() - hs, r.bottom() - hs, s, s),
            'b':  QRect(r.center().x() - hs, r.bottom() - hs, s, s),
            'bl': QRect(r.left() - hs, r.bottom() - hs, s, s),
            'l':  QRect(r.left() - hs, r.center().y() - hs, s, s),
        }

    def get_handle_at(self, pos):
        handles = self.get_handle_rects()
        for name, rect in handles.items():
            if rect.contains(pos):
                return name
        if self.selection_rect.contains(pos):
            return 'move'
        return None

    def mousePressEvent(self, event):
        if event.button() == Qt.LeftButton:
            handle = self.get_handle_at(event.pos())
            if handle:
                self.active_handle = handle
                self.drag_start_pos = event.pos()
                self.rect_start_geometry = self.selection_rect
                self.is_selecting = True
            else:
                self.active_handle = 'new'
                self.origin = event.pos()
                self.selection_rect = QRect(self.origin, QSize(0,0))
                self.is_selecting = True
                self.update()
        elif event.button() == Qt.RightButton:
            self.close()

    def mouseMoveEvent(self, event):
        if not self.is_selecting:
            # Update cursor based on hover
            handle = self.get_handle_at(event.pos())
            if handle in ['tl', 'br']: self.setCursor(Qt.SizeFDiagCursor)
            elif handle in ['tr', 'bl']: self.setCursor(Qt.SizeBDiagCursor)
            elif handle in ['t', 'b']: self.setCursor(Qt.SizeVerCursor)
            elif handle in ['l', 'r']: self.setCursor(Qt.SizeHorCursor)
            elif handle == 'move': self.setCursor(Qt.SizeAllCursor)
            else: self.setCursor(Qt.CrossCursor)
            return

        pos = event.pos()
        
        if self.active_handle == 'new':
            self.selection_rect = QRect(self.origin, pos).normalized()
        elif self.active_handle == 'move':
            delta = pos - self.drag_start_pos
            self.selection_rect = self.rect_start_geometry.translated(delta)
        else:
            # Resizing logic
            r = self.rect_start_geometry
            dx = pos.x() - self.drag_start_pos.x()
            dy = pos.y() - self.drag_start_pos.y()
            
            new_r = QRect(r)
            
            if 'l' in self.active_handle: new_r.setLeft(r.left() + dx)
            if 'r' in self.active_handle: new_r.setRight(r.right() + dx)
            if 't' in self.active_handle: new_r.setTop(r.top() + dy)
            if 'b' in self.active_handle: new_r.setBottom(r.bottom() + dy)
            
            self.selection_rect = new_r.normalized()
        
        self.update()

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.LeftButton:
            self.is_selecting = False
            self.selection_rect = self.selection_rect.normalized()
            self.update()

    def keyPressEvent(self, event):
        if event.key() == Qt.Key_Escape:
            self.close()
        elif event.key() == Qt.Key_Return or event.key() == Qt.Key_Enter:
            if not self.selection_rect.isNull():
                self.take_snippet()
            self.close()

    def take_snippet(self):
        snippet = self.full_pixmap.copy(self.selection_rect)
        clipboard = QApplication.clipboard()
        clipboard.setPixmap(snippet)
        print("Screenshot copied to clipboard.")

    def closeEvent(self, event):
        self.closed.emit()
        super().closeEvent(event)
