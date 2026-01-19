import tempfile
import os
from PyQt6.QtGui import QImage, QPixmap
from PyQt6.QtDBus import QDBusInterface, QDBusConnection, QDBusUnixFileDescriptor, QDBusMessage

class ScreenshotBackend:
    def _capture_internal(self, method_name, *args):
        try:
            # Create interface to KWin Screenshot2
            interface = QDBusInterface(
                'org.kde.KWin', 
                '/org/kde/KWin/ScreenShot2', 
                'org.kde.KWin.ScreenShot2', 
                QDBusConnection.sessionBus()
            )
            
            if not interface.isValid():
                print(f"Error: Invalid DBus interface for KWin Screenshot2: {interface.lastError().message()}")
                return None

            with tempfile.TemporaryFile() as tf:
                fd = tf.fileno()
                # QDBusUnixFileDescriptor takes a file descriptor
                dbus_fd = QDBusUnixFileDescriptor(fd)
                options = {'native-resolution': True}
                
                # Dynamic method call using QDBusInterface.callWithArgumentList or call
                # But call() handles variable args easily.
                
                # args need to be flattened: existing args + options + fd
                # However, QDBusInterface.call takes method name + args
                
                # method call signature:
                # CaptureWorkspace: (options, fd)
                # CaptureScreen: (screen_name, options, fd)
                
                call_args = list(args) + [options, dbus_fd]
                
                # Blocking call
                reply = interface.call(method_name, *call_args)
                
                if reply.type() == QDBusMessage.MessageType.ErrorMessage:
                    print(f"DBus Error ({method_name}): {reply.errorMessage()}")
                    return None
                    
                # Return value is the metadata dict
                # In PyQt6, it should be automatically converted to python dict
                # unless it returns a QDBusArgument?
                # Usually python types are handled.
                
                metadata = reply.arguments()[0]
                
                tf.seek(0)
                data = tf.read()

                if not data:
                    print(f"Error: Empty data received from {method_name}")
                    return None
                    
                # Handle metadata - it should be a dict maps string to variant
                if hasattr(metadata, 'value'): # if wrapped in QVariant/QDBusArgument?
                     # Debugging might be needed here if structure is complex
                     pass

                width = int(metadata.get('width', 0))
                height = int(metadata.get('height', 0))
                stride = int(metadata.get('stride', 0))
                
                if width <= 0 or height <= 0:
                    print(f"Invalid dimensions: {width}x{height}")
                    return None
                
                # QImage from buffer
                # Format_ARGB32 is 5 (?)
                img = QImage(data, width, height, stride, QImage.Format.Format_ARGB32)
                pixmap = QPixmap.fromImage(img.copy())
                
                return pixmap

        except Exception as e:
            print(f"Capture failed ({method_name}): {e}")
            import traceback
            traceback.print_exc()
            return None

    def capture_workspace(self):
        return self._capture_internal('CaptureWorkspace')
    
    def capture_screen(self, screen_name):
        return self._capture_internal('CaptureScreen', screen_name)
