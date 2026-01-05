import sys
import signal
from PyQt5.QtWidgets import QApplication, QSystemTrayIcon, QMenu, QAction
from PyQt5.QtCore import Qt
from PyQt5.QtGui import QIcon, QGuiApplication
from screenshot_tool import ScreenshotBackend, SnippingWidget

class ScreenshotApp:
    def __init__(self):
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
        
        # Keep track of active snipping widget
        self.snipper = None

    def on_tray_activated(self, reason):
        if reason == QSystemTrayIcon.Trigger:
            self.start_capture()

    def start_capture(self):
        print("Starting capture...")
        pixmap = self.backend.capture()
        if pixmap:
            # We want to cover all screens. 
            # In Wayland, creating a window that spans all screens is tricky.
            # But normally we just request full screen geometry.
            # For simplicity, let's grab the virtual geometry.
            desktop = self.app.desktop()
            geometry = desktop.geometry()
            
            # If we used CaptureActiveScreen, the pixmap might be smaller than full desktop 
            # if multiple monitors are present but we only captured one. 
            # However, SnippingWidget expects to draw the pixmap.
            
            # If pixmap size doesn't match virtual desktop, we might have an issue 
            # positioning it correctly relative to screens. 
            # But let's assume KWin gives us the right thing or we just show it on primary.
            
            # To handle multiple monitors properly with one giant overlay window often requires 
            # X11BypassWindowManagerHint (which we set) and correct geometry.
            
            self.snipper = SnippingWidget(pixmap, geometry.x(), geometry.y(), geometry.width(), geometry.height())
            self.snipper.show()
            self.snipper.activateWindow()
        else:
            print("Failed to capture screenshot.")

    def run(self):
        # Allow Ctrl+C to kill
        signal.signal(signal.SIGINT, signal.SIG_DFL)
        sys.exit(self.app.exec_())

if __name__ == "__main__":
    app = ScreenshotApp()
    app.run()
