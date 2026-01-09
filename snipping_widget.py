from PyQt5.QtQuickWidgets import QQuickWidget
from PyQt5.QtWidgets import QWidget
from PyQt5.QtCore import Qt, QRect, QSize, pyqtSignal, QTimer, QPoint, QUrl
from PyQt5.QtGui import QPainter, QColor, QBrush, QPen, QRegion

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
            
            # Draw annotations (Clipped to selection)
            painter.save()
            painter.setClipRect(local_sel)
            # Annotations are in GLOBAL coordinates, but painter is local (offset)
            # AnnotationManager expects painter to be in global coords?
            # Or we transform painter to global?
            # AnnotationItem.draw uses self.pos (Global).
            # So if we translate painter by -offset (which is -(-topLeft) = topLeft), we are in Global?
            # Wait, `offset` variable in code is `-self.window().screen_geometry.topLeft()`.
            # So `local = global + offset`.
            # So `global = local - offset`.
            # To draw in global coords, we need to apply transform `translate(offset)`.
            # Wait. `offset` is what we add to global to get local.
            # So if we have global points, and we want to draw them on this widget (which is at local (0,0)),
            # we need to translate the painter so that (0,0) becomes global (0,0)?
            # No, if I draw at (0,0) on widget, it's top-left of screen.
            # If I draw at global (100,100), and screen is at (0,0), it draws at (100,100).
            # If screen is at (1920,0), offset is (-1920, 0).
            # If I draw at global (1920, 0), I want it to appear at widget (0,0).
            # So I need to translate painter by `offset`. 
            # `painter.translate(offset.x(), offset.y())`.
            # Then drawing at 1920 will be at 1920-1920 = 0. Correct.
            painter.translate(offset)
            if hasattr(self.controller, 'annotation_manager'):
                self.controller.annotation_manager.draw_annotations(painter)
            painter.restore()
            
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
        self.setMouseTracking(True)
        
        # Layout container
        # We use absolute positioning for Toolbar (it floats), 
        # but SnippingWidget should fill the window.
        
        self.snipping_widget = SnippingWidget(controller, pixmap)
        self.snipping_widget.setParent(self)
        self.snipping_widget.resize(width, height)
        self.snipping_widget.move(0, 0)
        
        # Toolbar (QQuickWidget)
        self.toolbar = QQuickWidget(self)
        self.toolbar.setSource(QUrl.fromLocalFile("Toolbar.qml"))
        if self.toolbar.status() == QQuickWidget.Error:
             for error in self.toolbar.errors():
                 print("QML Error:", error.toString())
        
        self.toolbar.setResizeMode(QQuickWidget.SizeRootObjectToView)
        self.toolbar.setAttribute(Qt.WA_AlwaysStackOnTop)
        self.toolbar.setClearColor(Qt.transparent)
        self.toolbar.hide() # Hidden by default
        
        # Connect QML Signals
        root = self.toolbar.rootObject()
        if root:
             root.cancelRequested.connect(self.handle_cancel_or_exit)
             root.saveRequested.connect(lambda: print("Save requested (not implemented)"))
             root.confirmRequested.connect(self.handle_confirm_click)
             root.toolSelected.connect(self.handle_tool_selected)
        else:
             print("Error: Could not load QML root object")
        
        self.show()

    def resizeEvent(self, event):
        self.snipping_widget.resize(event.size())
        super().resizeEvent(event)

    def handle_tool_selected(self, tool_name):
        self.controller.set_tool(tool_name)

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
        elif event.key() == Qt.Key_Delete:
            self.controller.delete_selected_annotation()

    def update_toolbar_position(self):
        # Update Toolbar Position
        # Use controller to determine the unique snipper that should show the toolbar
        if not self.controller.selection_rect.isNull() and self == self.controller.get_active_toolbar_snipper():
             global_sel = self.controller.selection_rect
             # Convert global selection rect to local coordinates
             offset = -self.screen_geometry.topLeft()
             local_sel = global_sel.translated(offset)
             
             
             # Calculate position
             # Toolbar size comes from QML
             root_obj = self.toolbar.rootObject()
             if not root_obj:
                 return # Toolbar failed to load, skip positioning

             w = root_obj.width()
             h = root_obj.height()
             
             # Get top padding from QML (default to 0 if not property)
             top_padding = root_obj.property("topPadding")
             if top_padding is None:
                 top_padding = 0
             else:
                 top_padding = int(top_padding)
             
             # Adjust QQuickWidget size to match root object
             self.toolbar.resize(int(w), int(h))
             
             parent_rect = self.rect()
             
             # Logic from old toolbar widget:
             
             # Target X: Right aligned with selection
             x = local_sel.right() - w
             if x < parent_rect.left():
                 x = parent_rect.left()
             if x + w > parent_rect.left() + parent_rect.width():
                 x = parent_rect.left() + parent_rect.width() - w
                 
             # 1. Prefer Outside Bottom
             # We want the VISIBLE top of the toolbar to be at selection.bottom() + 1
             # Visible top is at y + top_padding
             # So y + top_padding = local_sel.bottom() + 1
             # y = local_sel.bottom() + 1 - top_padding
             
             y = local_sel.bottom() + 1 - top_padding
             if y + h <= parent_rect.bottom():
                  self.toolbar.move(int(x), int(y))
             else:
                  # 2. Prefer Outside Top
                  # We want the VISIBLE bottom of the toolbar to be at selection.top() - 1
                  # Visible bottom is at y + h (since h includes padding + visible content? No wait)
                  # In QML: height = visible + padding. Toolbar is at bottom.
                  # So Visible Bottom IS Widget Bottom.
                  # y + h = local_sel.top() - 1
                  # y = local_sel.top() - 1 - h
                  
                  y = local_sel.top() - 1 - h
                  if y >= parent_rect.top():
                      self.toolbar.move(int(x), int(y))
                  else:
                      # 3. Inside Bottom
                      # Visible Bottom at Selection Bottom
                      # y + h = target_bottom
                      target_bottom = min(local_sel.bottom(), parent_rect.bottom())
                      y = target_bottom - h
                      
                      # Ensure it doesn't go off top of screen
                      if y < parent_rect.top():
                          y = parent_rect.top()
                      self.toolbar.move(int(x), int(y))
             
             self.toolbar.show()
             self.toolbar.raise_() # Ensure toolbar is on top of snipping widget
        else:
             self.toolbar.hide()

    def closeEvent(self, event):
        self.closed.emit()
        super().closeEvent(event)

    def mousePressEvent(self, event):
        # Forward to snipping_widget if it didn't handle it directly
        # (This happens when Toolbar ignores the event)
        self.snipping_widget.mousePressEvent(event)

    def mouseReleaseEvent(self, event):
        self.snipping_widget.mouseReleaseEvent(event)

    def mouseDoubleClickEvent(self, event):
        self.snipping_widget.mouseDoubleClickEvent(event)

    def mouseMoveEvent(self, event):
        self.snipping_widget.mouseMoveEvent(event)
