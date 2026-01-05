import sys
import signal
from PyQt5.QtWidgets import QApplication, QSystemTrayIcon, QMenu, QAction
from PyQt5.QtCore import Qt, QObject
from PyQt5.QtGui import QIcon, QGuiApplication
from screenshot_backend import ScreenshotBackend
from snipping_widget import SnippingWidget

class ScreenshotApp(QObject):
    def __init__(self):
        super().__init__()
        # Enable High DPI scaling
        if hasattr(Qt, 'AA_EnableHighDpiScaling'):
            QApplication.setAttribute(Qt.AA_EnableHighDpiScaling)
        if hasattr(Qt, 'AA_UseHighDpiPixmaps'):
            QApplication.setAttribute(Qt.AA_UseHighDpiPixmaps)
            
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

    def on_tray_activated(self, reason):
        if reason == QSystemTrayIcon.Trigger:
            self.start_capture()

    def start_capture(self):
        print("Starting capture...")
        self.close_all_snippers()
        
        screens = QGuiApplication.screens()
        if not screens:
            print("No screens found")
            return

        # Attempt per-screen capture first (best for multi-monitor/HiDPI)
        screen_pixmaps = {}
        all_success = True
        
        for screen in screens:
            # We assume screen.name() acts as the identifier KWin expects
            p = self.backend.capture_screen(screen.name())
            if p:
                p.setDevicePixelRatio(screen.devicePixelRatio())
                screen_pixmaps[screen] = p
            else:
                all_success = False
                break
        
        if all_success:
            print("Used per-screen capture.")
            for screen, pixmap in screen_pixmaps.items():
                geo = screen.geometry()
                snipper = SnippingWidget(pixmap, geo.x(), geo.y(), geo.width(), geo.height())
                snipper.selection_started.connect(self.on_selection_started)
                snipper.closed.connect(self.on_snipper_closed)
                
                if snipper.windowHandle():
                    snipper.windowHandle().setScreen(screen)
                    
                snipper.show()
                self.snippers.append(snipper)
            return

        # Fallback to workspace capture (stitched)
        print("Per-screen capture failed/incomplete. Falling back to workspace capture.")
        pixmap = self.backend.capture_workspace()
        if not pixmap:
            print("Failed to capture screenshot.")
            return

        # Note: Slicing a stitched workspace pixmap correctly with mixed DPI is difficult 
        # without knowing how KWin stitches them physically.
        # We will try a best-effort slicing based on logical geometry, 
        # but we won't set a global DPR on the source pixmap.
        
        # Calculate bounding box of all screens to find offsets
        x_min = min(s.geometry().x() for s in screens)
        y_min = min(s.geometry().y() for s in screens)
        
        for screen in screens:
            geo = screen.geometry()
            dpr = screen.devicePixelRatio()
            
            # Heuristic: KWin usually stitches top-left aligned, scaled by DPR.
            # But "geo" is logical.
            # Let's try slicing based on Screen geometry assuming 1:1 if we can't do better.
            # OR we try to slice assuming the workspace image matches the composite logical layout * MaxDPR?
            # Actually, standard KWin behavior for 'native-resolution' returns full physical size.
            # If we just cut based on (x - min_x) * dpr ... ? This is a guess.
            
            # Let's try to just map logical coordinates.
            rel_x = geo.x() - x_min
            rel_y = geo.y() - y_min
            
            # Since pixmap is physical, we need to scale logical coordinates to physical
            # BUT this only works if the stitching preserves this mapping.
            # If this fallback path is hit, the result might be imperfect, but 'CaptureScreen' should succeed on KWin.
            
            # Using logical slicing on physical image often results in small top-left crop on HiDPI.
            # So we MUST scale.
            
            phy_x = int(rel_x * dpr)
            phy_y = int(rel_y * dpr)
            phy_w = int(geo.width() * dpr)
            phy_h = int(geo.height() * dpr)
            
            screen_pixmap = pixmap.copy(phy_x, phy_y, phy_w, phy_h)
            screen_pixmap.setDevicePixelRatio(dpr)
            
            snipper = SnippingWidget(screen_pixmap, geo.x(), geo.y(), geo.width(), geo.height())
            snipper.selection_started.connect(self.on_selection_started)
            snipper.closed.connect(self.on_snipper_closed)
            
            if snipper.windowHandle():
                snipper.windowHandle().setScreen(screen)
            
            snipper.show()
            self.snippers.append(snipper)

    def on_selection_started(self):
        sender = self.sender()
        for snipper in self.snippers:
            if snipper != sender:
                snipper.clear_selection()

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
