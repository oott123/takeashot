import sys
import signal
from PyQt5.QtWidgets import QApplication, QSystemTrayIcon, QMenu, QAction
from PyQt5.QtCore import Qt, QObject, QRect, QPoint, QSize, QRectF
from PyQt5.QtGui import QIcon, QGuiApplication, QPixmap, QPainter, QImage
from screenshot_backend import ScreenshotBackend
from snipping_widget import SnippingWidget
from window_lister import WindowLister
from annotations import FreehandTool, RectangleTool, LineTool
from toolbar import AnnotationToolbar

# Enable High DPI scaling - MUST be set before QApplication creation
if hasattr(Qt, 'AA_EnableHighDpiScaling'):
    QApplication.setAttribute(Qt.AA_EnableHighDpiScaling)
if hasattr(Qt, 'AA_UseHighDpiPixmaps'):
    QApplication.setAttribute(Qt.AA_UseHighDpiPixmaps)

class ScreenshotApp(QObject):
    def __init__(self):
        super().__init__()
        self.app = QApplication(sys.argv)
        self.app.setQuitOnLastWindowClosed(False)
        
        # Setup Backend
        self.backend = ScreenshotBackend()
        
        # Setup System Tray
        self.tray_icon = QSystemTrayIcon(self.app)
        
        # Try to set a system icon
        icon = QIcon.fromTheme("camera-photo")
        if icon.isNull():
            icon = QIcon.fromTheme("accessories-screenshot")
        if icon.isNull():
            # Fallback: create a simple icon programmatically if needed, or use standard icon
             icon = self.app.style().standardIcon(self.app.style().SP_ComputerIcon)
             
        self.tray_icon.setIcon(icon)
        self.tray_icon.setToolTip("Take a Shot")
        
        # Menu
        menu = QMenu()
        action_take = QAction("Take Screenshot", self.app)
        action_take.triggered.connect(self.start_capture)
        menu.addAction(action_take)
        
        action_quit = QAction("Quit", self.app)
        action_quit.triggered.connect(self.app.quit)
        menu.addAction(action_quit)
        
        self.tray_icon.setContextMenu(menu)
        self.tray_icon.activated.connect(self.on_tray_activated)
        
        self.tray_icon.show()
        
        # Keep track of active snipping widgets
        self.snippers = []

        # Global Selection State
        self.selection_rect = QRect()
        self.is_selecting = False
        self.active_handle = None
        self.drag_start_pos = QPoint()
        self.rect_start_geometry = QRect()
        self.origin = QPoint()
        
        # Mouse tracking for click vs drag detection
        self.is_dragging = False
        self.click_start_pos = QPoint()
        self.MOUSE_DRAG_THRESHOLD = 5
        
        # Window snapping state
        self.windows = []  # List of window dictionaries from KWin
        self.pending_window = None  # Window under mouse (snapping preview)
        self.pending_selection_rect = QRect()  # Pending (snap) selection rectangle (not real selection)
        self.window_lister = None  # Window lister instance
        self.snapping_enabled = False  # Whether window snapping is active

        # Constants
        self.RESIZE_HANDLE_SIZE = 8

        # Annotation state
        self.tool_manager = ToolManager()
        self.toolbar = AnnotationToolbar()
        self.toolbar.tool_selected.connect(self.on_tool_selected)
        self.toolbar.clear_requested.connect(self.on_clear_annotations)

    def on_tray_activated(self, reason):
        if reason == QSystemTrayIcon.Trigger:
            self.start_capture()

    def start_capture(self):
        print("Starting capture...")
        self.close_all_snippers()
        
        # Reset selection state
        self.selection_rect = QRect()
        self.is_selecting = False
        self.active_handle = None
        self.is_dragging = False
        self.click_start_pos = QPoint()
        
        screens = QGuiApplication.screens()
        if not screens:
            print("No screens found")
            return

        # Attempt per-screen capture first (best for multi-monitor/HiDPI)
        screen_pixmaps = {}
        all_success = True
        
        for screen in screens:
            p = self.backend.capture_screen(screen.name())
            if p:
                # Calculate the ACTUAL DPR based on physical pixels vs logical geometry
                # This is more reliable than screen.devicePixelRatio() which may return 1.0 
                # in some XWayland/KDE configurations while the screenshot is native.
                geo = screen.geometry()
                phys_w = p.width()
                logic_w = geo.width()
                
                # Use the ratio of physical to logical
                actual_dpr = phys_w / logic_w if logic_w > 0 else 1.0
                p.setDevicePixelRatio(actual_dpr)
                
                print(f"Captured {screen.name()}: Logical {logic_w}x{geo.height()}, Physical {phys_w}x{p.height()}, DPR {actual_dpr}")
                screen_pixmaps[screen] = p
            else:
                all_success = False
                break
        
        if all_success:
            print("Used per-screen capture.")
            self._launch_snippers(screen_pixmaps)
            self._start_window_snapping()
            return

        # Fallback to workspace capture (stitched)
        print("Per-screen capture failed/incomplete. Falling back to workspace capture.")
        pixmap = self.backend.capture_workspace()
        if not pixmap:
            print("Failed to capture screenshot.")
            return

        screen_pixmaps = {}
        # Calculate bounding box of all screens to find offsets
        x_min = min(s.geometry().x() for s in screens)
        y_min = min(s.geometry().y() for s in screens)
        
        for screen in screens:
            geo = screen.geometry()
            dpr = screen.devicePixelRatio()
            
            rel_x = geo.x() - x_min
            rel_y = geo.y() - y_min
            
            phy_x = int(rel_x * dpr)
            phy_y = int(rel_y * dpr)
            phy_w = int(geo.width() * dpr)
            phy_h = int(geo.height() * dpr)
            
            screen_pixmap = pixmap.copy(phy_x, phy_y, phy_w, phy_h)
            screen_pixmap.setDevicePixelRatio(dpr)
            screen_pixmaps[screen] = screen_pixmap

        self._launch_snippers(screen_pixmaps)
        self._start_window_snapping()

    def _launch_snippers(self, screen_pixmaps):
        for screen, pixmap in screen_pixmaps.items():
            geo = screen.geometry()
            snipper = SnippingWidget(self, pixmap, geo.x(), geo.y(), geo.width(), geo.height())
            snipper.closed.connect(self.on_snipper_closed)
            
            if snipper.windowHandle():
                snipper.windowHandle().setScreen(screen)
                
            snipper.show()
            self.snippers.append(snipper)

    def _start_window_snapping(self):
        """启动异步窗口列表获取，启用窗口吸附功能"""
        print("Starting window list retrieval for snapping...")
        self.window_lister = WindowLister()
        self.window_lister.windows_ready.connect(self._on_windows_ready)
        self.window_lister.get_windows_async()

    def _on_windows_ready(self, windows):
        """窗口列表获取完成回调"""
        if windows:
            self.windows = windows
            self.snapping_enabled = True
            print(f"Window snapping enabled with {len(windows)} windows")
        else:
            self.windows = []
            self.snapping_enabled = False
            print("Window snapping disabled (no windows retrieved)")

    def _get_window_at(self, global_pos):
        """获取鼠标位置下的窗口，如果没有则返回None"""
        if not self.windows:
            return None
        
        for window in self.windows:
            # Convert coordinates to integers (KWin may return floats)
            x, y, w, h = int(window['x']), int(window['y']), int(window['width']), int(window['height'])
            window_rect = QRect(x, y, w, h)
            if window_rect.contains(global_pos):
                return window
        return None

    def capture_selection(self):
        if self.selection_rect.isNull():
            return

        # Normalize selection to handle any-direction drag
        sel_rect = self.selection_rect.normalized()
        
        # Determine all intersecting screens and the target max DPR
        intersecting_data = []
        max_dpr = 1.0
        
        for snipper in self.snippers:
            inter = snipper.screen_geometry.intersected(sel_rect)
            if not inter.isEmpty():
                dpr = snipper.full_pixmap.devicePixelRatio()
                intersecting_data.append((snipper, inter, dpr))
                if dpr > max_dpr:
                    max_dpr = dpr

        if not intersecting_data:
            return

        # Target size in physical pixels
        target_w_phys = int(round(sel_rect.width() * max_dpr))
        target_h_phys = int(round(sel_rect.height() * max_dpr))
        
        if target_w_phys <= 0 or target_h_phys <= 0:
            return

        # Create result image in physical pixels (NO devicePixelRatio set - we work in raw pixels)
        result_img = QImage(target_w_phys, target_h_phys, QImage.Format_ARGB32)
        result_img.fill(Qt.transparent)

        painter = QPainter(result_img)
        painter.setRenderHint(QPainter.SmoothPixmapTransform)
        
        for snipper, inter, s_dpr in intersecting_data:
            # inter is the intersection rectangle in GLOBAL LOGICAL coordinates
            # We need to:
            # 1. Find the source region in the snipper's pixmap (in physical pixels)
            # 2. Find the target region in result_img (in physical pixels)
            
            # Source: offset from snipper's screen origin, scaled by that screen's DPR
            src_x = (inter.x() - snipper.screen_geometry.x()) * s_dpr
            src_y = (inter.y() - snipper.screen_geometry.y()) * s_dpr
            src_w = inter.width() * s_dpr
            src_h = inter.height() * s_dpr
            
            # Target: offset from selection origin, scaled by max_dpr
            tgt_x = (inter.x() - sel_rect.x()) * max_dpr
            tgt_y = (inter.y() - sel_rect.y()) * max_dpr
            tgt_w = inter.width() * max_dpr
            tgt_h = inter.height() * max_dpr
            
            source_rect = QRectF(src_x, src_y, src_w, src_h)
            target_rect = QRectF(tgt_x, tgt_y, tgt_w, tgt_h)
            
            # Convert pixmap to QImage to bypass DPR interpretation issues
            # QPixmap.toImage() gives raw physical pixels
            src_image = snipper.full_pixmap.toImage()

            painter.drawImage(target_rect, src_image, source_rect)

        # Render annotations onto the screenshot
        painter.setRenderHint(QPainter.Antialiasing)
        offset = -sel_rect.topLeft()
        self.tool_manager.render_annotations(painter, offset, sel_rect)

        painter.end()

        # Convert to pixmap for clipboard
        result_pixmap = QPixmap.fromImage(result_img)
        # Set DPR so apps that understand it can display at correct size
        result_pixmap.setDevicePixelRatio(max_dpr)

        clipboard = QApplication.clipboard()
        clipboard.setPixmap(result_pixmap)
        print(f"Captured {sel_rect.width()}x{sel_rect.height()} logical ({target_w_phys}x{target_h_phys} physical) to clipboard.")

    def on_mouse_press(self, global_pos):
        # Check if this is an annotation event
        if self.is_annotation_event(global_pos):
            self.tool_manager.start_annotation(global_pos, self.selection_rect)
            self.update_snippets()
            return

        # Record press position for click vs drag detection
        self.click_start_pos = global_pos
        self.is_dragging = False
        self.is_selecting = True  # Track that mouse is down, but don't know if drag yet

        # Store initial state in case it becomes a drag
        self.drag_start_pos = global_pos
        self.rect_start_geometry = QRect(self.selection_rect)

        # Check for resize handles (ONLY if we have real selection) - FIX BUG
        if not self.selection_rect.isNull():
            handle = self.get_handle_at(global_pos)
            if handle:
                self.active_handle = handle
                return

        # Determine action based on current state
        if not self.pending_selection_rect.isNull():
            # Pending selection state
            if self.pending_selection_rect.contains(global_pos):
                # Clicked inside pending - will confirm on mouseup if it's a click
                self.active_handle = 'confirm_pending'
            else:
                # Clicked outside pending - will start new selection on drag
                self.active_handle = 'new'
                self.origin = global_pos
        elif not self.selection_rect.isNull():
            # Has selection state
            if self.selection_rect.contains(global_pos):
                # Clicked inside selection - will move on drag
                self.active_handle = 'move'
            else:
                # Clicked outside selection - expand selection to include the point
                self.expand_selection_to_point(global_pos)
        else:
            # No selection state
            # Will start new selection on drag
            self.active_handle = 'new'
            self.origin = global_pos

        self.update_snippets()

    def on_mouse_move(self, global_pos):
        # Handle annotation drawing
        if self.tool_manager.is_drawing:
            self.tool_manager.update_annotation(global_pos, self.selection_rect)
            self.update_snippets()
            return

        if not self.is_selecting:
            # Window snapping: if no real selection and snapping enabled, check if mouse is over a window
            if self.snapping_enabled and self.selection_rect.isNull():
                snapped_window = self._get_window_at(global_pos)
                if snapped_window and snapped_window != self.pending_window:
                    # Set pending selection to window geometry (snapping preview)
                    self.pending_window = snapped_window
                    # Convert coordinates to integers
                    x, y, w, h = int(snapped_window['x']), int(snapped_window['y']), int(snapped_window['width']), int(snapped_window['height'])
                    self.pending_selection_rect = QRect(x, y, w, h)
                    self.update_snippets()
                elif not snapped_window and self.pending_window:
                    # Mouse left the window, clear preview
                    self.pending_window = None
                    self.pending_selection_rect = QRect()
                    self.update_snippets()
            return

        # Calculate distance from click start
        distance = (global_pos - self.click_start_pos).manhattanLength()

        # Check if movement exceeds threshold (becomes a drag)
        if not self.is_dragging and distance > self.MOUSE_DRAG_THRESHOLD:
            self.is_dragging = True
            # Clear pending selection when starting a real drag
            if self.pending_window:
                self.pending_window = None
                self.pending_selection_rect = QRect()
            # If we were going to confirm pending but now dragging, start new selection instead
            if self.active_handle == 'confirm_pending':
                self.active_handle = 'new'
                self.origin = self.click_start_pos

        # Only process drag operations if we've exceeded threshold
        if self.is_dragging:
            if self.active_handle == 'new':
                # Creating new selection
                self.selection_rect = QRect(self.origin, global_pos).normalized()
            elif self.active_handle == 'move':
                # Moving existing selection
                delta = global_pos - self.drag_start_pos
                self.selection_rect = self.rect_start_geometry.translated(delta)
            elif self.active_handle in ['tl', 't', 'tr', 'r', 'br', 'b', 'bl', 'l']:
                # Resizing selection
                r = self.rect_start_geometry
                dx = global_pos.x() - self.drag_start_pos.x()
                dy = global_pos.y() - self.drag_start_pos.y()

                new_r = QRect(r)

                if 'l' in self.active_handle: new_r.setLeft(r.left() + dx)
                if 'r' in self.active_handle: new_r.setRight(r.right() + dx)
                if 't' in self.active_handle: new_r.setTop(r.top() + dy)
                if 'b' in self.active_handle: new_r.setBottom(r.bottom() + dy)

                self.selection_rect = new_r.normalized()
            # 'confirm_pending' handle does nothing during drag (cleared when drag starts)

            self.update_snippets()

    def on_mouse_release(self):
        # Finish annotation if drawing
        if self.tool_manager.is_drawing:
            self.tool_manager.finish_annotation(self.drag_start_pos, self.selection_rect)
            self.update_snippets()
            return

        # Determine if this was a click or a drag
        distance = (self.drag_start_pos - self.click_start_pos).manhattanLength()
        was_drag = distance > self.MOUSE_DRAG_THRESHOLD

        if not was_drag:
            # This was a CLICK - execute click-based actions based on state
            if self.active_handle == 'confirm_pending':
                # Pending selection state: clicked inside pending - confirm it
                self.selection_rect = QRect(self.pending_selection_rect)
                self.pending_selection_rect = QRect()
                self.pending_window = None
            elif self.active_handle == 'move':
                # Has selection state: clicked inside selection - no action
                pass
            # All other click scenarios do nothing (no action)
        else:
            # This was a DRAG - finalize the drag operation
            self.selection_rect = self.selection_rect.normalized()

        # Reset selection state
        self.is_selecting = False
        self.is_dragging = False
        self.active_handle = None

        self.update_snippets()

    def update_snippets(self):
        for snipper in self.snippers:
            snipper.update()
            # Update toolbar visibility (only need to do this once, but update all for simplicity)
            snipper.update_toolbar_visibility()

    def get_handle_rects(self):
        # Only show handles for real selection, not pending selection
        if self.selection_rect.isNull():
            return {}
            
        r = self.selection_rect
        s = self.RESIZE_HANDLE_SIZE
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

    def get_handle_at(self, global_pos):
        # Only allow handle interaction for real selection
        if self.selection_rect.isNull():
            return None
            
        handles = self.get_handle_rects()
        for name, rect in handles.items():
            if rect.contains(global_pos):
                return name
        if self.selection_rect.contains(global_pos):
            return 'move'
        return None
    
    def expand_selection_to_point(self, point):
        """
        将选区扩大到包含指定点
        
        扩大策略：
        1. 如果扩大一个方向就能覆盖点，则只扩大一个方向
        2. 如果扩大一个方向不能覆盖，则扩大两个方向
        """
        if self.selection_rect.isNull():
            return
        
        r = self.selection_rect
        
        # 检查点是否在选区内
        if r.contains(point):
            return
        
        # 计算新的边界
        new_left = r.left()
        new_right = r.right()
        new_top = r.top()
        new_bottom = r.bottom()
        
        # 根据点的位置决定扩大哪些方向
        if point.x() < r.left():
            new_left = point.x()
        elif point.x() > r.right():
            new_right = point.x()
        
        if point.y() < r.top():
            new_top = point.y()
        elif point.y() > r.bottom():
            new_bottom = point.y()
        
        # 创建新的选区
        self.selection_rect = QRect(new_left, new_top, 
                                     new_right - new_left, 
                                     new_bottom - new_top)
        
        # 更新界面
        self.update_snippets()
    
    def get_pending_selection_rect(self):
        """获取拟选中矩形"""
        return self.pending_selection_rect

    def on_snipper_closed(self):
        if self.snippers:
            self.close_all_snippers()

    def close_all_snippers(self):
        if not self.snippers:
            return
        
        current_snippers = self.snippers
        self.snippers = []
        
        for snipper in current_snippers:
            snipper.close()

    def cancel_selection(self):
        """取消当前选区，返回是否成功取消"""
        # Cancel real selection first
        if not self.selection_rect.isNull():
            self.selection_rect = QRect()
            self.tool_manager.clear_all()  # Clear annotations
            self.update_snippets()
            return True

        # If no real selection, cancel pending selection
        if not self.pending_selection_rect.isNull():
            self.pending_selection_rect = QRect()
            self.pending_window = None
            self.update_snippets()
            return True

        return False

    def on_tool_selected(self, tool_type: str) -> None:
        """Handle tool selection from toolbar."""
        self.tool_manager.set_tool(tool_type)

    def on_clear_annotations(self) -> None:
        """Handle clear annotations request."""
        self.tool_manager.clear_all()
        self.update_snippets()

    def has_active_tool(self) -> bool:
        """Check if an annotation tool is active."""
        return self.tool_manager.tool_type is not None

    def is_annotation_event(self, global_pos: QPoint) -> bool:
        """Check if mouse event should be handled as annotation."""
        return (self.has_active_tool() and
                not self.selection_rect.isNull() and
                self.selection_rect.contains(global_pos))

    def should_exit(self):
        """判断是否应退出截图（无选区且无拟选中）"""
        return self.selection_rect.isNull() and self.pending_selection_rect.isNull()

    def run(self):
        # Allow Ctrl+C to kill
        signal.signal(signal.SIGINT, signal.SIG_DFL)
        sys.exit(self.app.exec_())


class ToolManager:
    """Manages annotation tools and state."""

    def __init__(self):
        self.current_tool = None  # Active tool instance
        self.tool_type = None  # 'freehand', 'rectangle', 'line'
        self.annotations = []  # List of completed annotations
        self.is_drawing = False  # Currently drawing

    def set_tool(self, tool_type: str) -> None:
        """Set the active tool type."""
        self.tool_type = tool_type
        self.current_tool = None  # Reset current instance

    def clear_all(self) -> None:
        """Clear all annotations."""
        self.annotations.clear()
        self.current_tool = None
        self.is_drawing = False

    def start_annotation(self, pos: QPoint, selection_rect: QRect) -> None:
        """Start a new annotation."""
        if not self.tool_type:
            return

        # Create new tool instance
        if self.tool_type == 'freehand':
            self.current_tool = FreehandTool()
        elif self.tool_type == 'rectangle':
            self.current_tool = RectangleTool()
        elif self.tool_type == 'line':
            self.current_tool = LineTool()

        if self.current_tool:
            self.current_tool.on_mouse_press(pos, selection_rect)
            self.is_drawing = True

    def update_annotation(self, pos: QPoint, selection_rect: QRect) -> None:
        """Update current annotation."""
        if self.current_tool and self.is_drawing:
            self.current_tool.on_mouse_move(pos, selection_rect)

    def finish_annotation(self, pos: QPoint, selection_rect: QRect) -> None:
        """Finish current annotation."""
        if self.current_tool and self.is_drawing:
            self.current_tool.on_mouse_release(pos, selection_rect)
            if self.current_tool.is_complete():
                self.annotations.append(self.current_tool)
            self.current_tool = None
            self.is_drawing = False

    def render_annotations(self, painter: QPainter, offset: QPoint, selection_rect: QRect) -> None:
        """Render all annotations."""
        # Render completed annotations
        for annotation in self.annotations:
            annotation.paint(painter, offset, selection_rect)

        # Render current annotation being drawn
        if self.current_tool:
            self.current_tool.paint(painter, offset, selection_rect)

if __name__ == "__main__":
    app = ScreenshotApp()
    app.run()
