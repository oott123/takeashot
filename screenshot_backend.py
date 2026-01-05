import dbus
import tempfile
from PyQt5.QtGui import QImage, QPixmap

class ScreenshotBackend:
    def _capture_internal(self, method_name, *args):
        try:
            bus = dbus.SessionBus()
            obj = bus.get_object('org.kde.KWin', '/org/kde/KWin/ScreenShot2')
            interface = dbus.Interface(obj, 'org.kde.KWin.ScreenShot2')

            with tempfile.TemporaryFile() as tf:
                fd = tf.fileno()
                dbus_fd = dbus.types.UnixFd(fd)
                options = {'native-resolution': True}
                
                method = getattr(interface, method_name)
                # args need to be prepended to options, fd
                # expected signature varies.
                # CaptureWorkspace: (options, fd)
                # CaptureScreen: (screen_name, options, fd)
                
                call_args = list(args) + [options, dbus_fd]
                metadata = method(*call_args)

                tf.seek(0)
                data = tf.read()

                if not data:
                    print(f"Error: Empty data received from {method_name}")
                    return None

                width = int(metadata.get('width', 0))
                height = int(metadata.get('height', 0))
                stride = int(metadata.get('stride', 0))
                
                if width <= 0 or height <= 0:
                    print(f"Invalid dimensions: {width}x{height}")
                    return None
                
                img = QImage(data, width, height, stride, QImage.Format_ARGB32)
                pixmap = QPixmap.fromImage(img.copy())
                # DO NOT set device pixel ratio here globally. Let caller handle it.
                
                return pixmap

        except Exception as e:
            print(f"Capture failed ({method_name}): {e}")
            return None

    def capture_workspace(self):
        return self._capture_internal('CaptureWorkspace')
    
    def capture_screen(self, screen_name):
        return self._capture_internal('CaptureScreen', screen_name)
