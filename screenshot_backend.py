import tempfile
import os
import time
import logging
from PyQt6.QtGui import QImage, QPixmap
from PyQt6.QtDBus import QDBusInterface, QDBusConnection, QDBusUnixFileDescriptor, QDBusMessage

logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(name)s - %(levelname)s - %(message)s')

class ScreenshotBackend:
    def __init__(self):
        self.logger = logging.getLogger(__name__)
    def _capture_internal(self, method_name, *args):
        start_time = time.time()
        self.logger.info(f"Starting {method_name} capture")
        
        try:
            # Create interface to KWin Screenshot2
            interface_start = time.time()
            interface = QDBusInterface(
                'org.kde.KWin', 
                '/org/kde/KWin/ScreenShot2', 
                'org.kde.KWin.ScreenShot2', 
                QDBusConnection.sessionBus()
            )
            interface_time = time.time() - interface_start
            self.logger.info(f"DBus interface creation took {interface_time:.3f}s")
            
            if not interface.isValid():
                self.logger.error(f"Invalid DBus interface for KWin Screenshot2: {interface.lastError().message()}")
                return None

            with tempfile.TemporaryFile() as tf:
                temp_file_start = time.time()
                fd = tf.fileno()
                # QDBusUnixFileDescriptor takes a file descriptor
                dbus_fd = QDBusUnixFileDescriptor(fd)
                options = {'native-resolution': True}
                temp_file_time = time.time() - temp_file_start
                self.logger.info(f"Temporary file setup took {temp_file_time:.3f}s")
                
                # Dynamic method call using QDBusInterface.callWithArgumentList or call
                # But call() handles variable args easily.
                
                # args need to be flattened: existing args + options + fd
                # However, QDBusInterface.call takes method name + args
                
                # method call signature:
                # CaptureWorkspace: (options, fd)
                # CaptureScreen: (screen_name, options, fd)
                
                call_args = list(args) + [options, dbus_fd]
                
                # Blocking call
                dbus_call_start = time.time()
                reply = interface.call(method_name, *call_args)
                dbus_call_time = time.time() - dbus_call_start
                self.logger.info(f"DBus call took {dbus_call_time:.3f}s")
                
                if reply.type() == QDBusMessage.MessageType.ErrorMessage:
                    self.logger.error(f"DBus Error ({method_name}): {reply.errorMessage()}")
                    return None
                    
                # Return value is the metadata dict
                # In PyQt6, it should be automatically converted to python dict
                # unless it returns a QDBusArgument?
                # Usually python types are handled.
                
                metadata = reply.arguments()[0]
                
                data_read_start = time.time()
                tf.seek(0)
                data = tf.read()
                data_read_time = time.time() - data_read_start
                self.logger.info(f"Data read took {data_read_time:.3f}s")

                if not data:
                    self.logger.error(f"Empty data received from {method_name}")
                    return None
                    
                # Handle metadata - it should be a dict maps string to variant
                if hasattr(metadata, 'value'): # if wrapped in QVariant/QDBusArgument?
                     # Debugging might be needed here if structure is complex
                     pass

                metadata_parse_start = time.time()
                width = int(metadata.get('width', 0))
                height = int(metadata.get('height', 0))
                stride = int(metadata.get('stride', 0))
                metadata_parse_time = time.time() - metadata_parse_start
                self.logger.info(f"Metadata parsing took {metadata_parse_time:.3f}s")
                
                if width <= 0 or height <= 0:
                    self.logger.error(f"Invalid dimensions: {width}x{height}")
                    return None
                
                # QImage from buffer
                # Format_ARGB32 is 5 (?)
                image_creation_start = time.time()
                img = QImage(data, width, height, stride, QImage.Format.Format_ARGB32)
                pixmap = QPixmap.fromImage(img.copy())
                image_creation_time = time.time() - image_creation_start
                self.logger.info(f"Image creation took {image_creation_time:.3f}s")
                
                total_time = time.time() - start_time
                self.logger.info(f"Total {method_name} capture took {total_time:.3f}s")
                return pixmap

        except Exception as e:
            self.logger.error(f"Capture failed ({method_name}): {e}")
            import traceback
            self.logger.error(traceback.format_exc())
            return None

    def capture_workspace(self):
        return self._capture_internal('CaptureWorkspace')
    
    def capture_screen(self, screen_name):
        return self._capture_internal('CaptureScreen', screen_name)
