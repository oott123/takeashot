import sys
import signal
from PyQt5.QtWidgets import QApplication, QSystemTrayIcon, QMenu, QAction
from PyQt5.QtCore import Qt, QObject, QRect, QPoint, QSize, QRectF
from PyQt5.QtGui import QIcon, QGuiApplication, QPixmap, QPainter, QImage
from screenshot_backend import ScreenshotBackend
from snipping_widget import SnippingWidget

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

        # Constants
        self.RESIZE_HANDLE_SIZE = 8

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

    def _launch_snippers(self, screen_pixmaps):
        for screen, pixmap in screen_pixmaps.items():
            geo = screen.geometry()
            snipper = SnippingWidget(self, pixmap, geo.x(), geo.y(), geo.width(), geo.height())
            snipper.closed.connect(self.on_snipper_closed)
            
            if snipper.windowHandle():
                snipper.windowHandle().setScreen(screen)
                
            snipper.show()
            self.snippers.append(snipper)

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

        painter.end()

        # Convert to pixmap for clipboard
        result_pixmap = QPixmap.fromImage(result_img)
        # Set DPR so apps that understand it can display at correct size
        result_pixmap.setDevicePixelRatio(max_dpr)

        clipboard = QApplication.clipboard()
        clipboard.setPixmap(result_pixmap)
        print(f"Captured {sel_rect.width()}x{sel_rect.height()} logical ({target_w_phys}x{target_h_phys} physical) to clipboard.")

    def on_mouse_press(self, global_pos):
        # Check for resize handles first
        handle = self.get_handle_at(global_pos)
        if handle:
            self.active_handle = handle
            self.drag_start_pos = global_pos
            self.rect_start_geometry = self.selection_rect
            self.is_selecting = True
        else:
            # Start new selection
            self.active_handle = 'new'
            self.origin = global_pos
            self.selection_rect = QRect(self.origin, QSize(0,0))
            self.is_selecting = True
        self.update_snippets()

    def on_mouse_move(self, global_pos):
        if not self.is_selecting:
            return

        if self.active_handle == 'new':
            self.selection_rect = QRect(self.origin, global_pos).normalized()
        elif self.active_handle == 'move':
            delta = global_pos - self.drag_start_pos
            self.selection_rect = self.rect_start_geometry.translated(delta)
        else:
            # Resizing logic
            r = self.rect_start_geometry
            dx = global_pos.x() - self.drag_start_pos.x()
            dy = global_pos.y() - self.drag_start_pos.y()
            
            new_r = QRect(r)
            
            if 'l' in self.active_handle: new_r.setLeft(r.left() + dx)
            if 'r' in self.active_handle: new_r.setRight(r.right() + dx)
            if 't' in self.active_handle: new_r.setTop(r.top() + dy)
            if 'b' in self.active_handle: new_r.setBottom(r.bottom() + dy)
            
            self.selection_rect = new_r.normalized()
        
        self.update_snippets()

    def on_mouse_release(self):
        self.is_selecting = False
        self.selection_rect = self.selection_rect.normalized()
        self.update_snippets()

    def update_snippets(self):
        for snipper in self.snippers:
            snipper.update()

    def get_handle_rects(self):
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
        if self.selection_rect.isNull():
            return None
            
        handles = self.get_handle_rects()
        for name, rect in handles.items():
            if rect.contains(global_pos):
                return name
        if self.selection_rect.contains(global_pos):
            return 'move'
        return None

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

    def run(self):
        # Allow Ctrl+C to kill
        signal.signal(signal.SIGINT, signal.SIG_DFL)
        sys.exit(self.app.exec_())

if __name__ == "__main__":
    app = ScreenshotApp()
    app.run()
