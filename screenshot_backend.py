import dbus
import tempfile
import time
import logging
from PIL import Image
from PyQt6.QtGui import QPixmap, QImage

logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(name)s - %(levelname)s - %(message)s')

class ScreenshotBackend:
    def __init__(self):
        self.logger = logging.getLogger(__name__)
    
    def _capture_internal(self, method_name, *args):
        start_time = time.time()
        self.logger.info(f"Starting {method_name} capture")
        
        try:
            # Connect to Session Bus
            bus_start = time.time()
            bus = dbus.SessionBus()
            self.logger.info(f"Connected to DBus session bus in {time.time() - bus_start:.3f}s")
            
            # Get the KWin Screenshot2 object
            obj_start = time.time()
            obj = bus.get_object('org.kde.KWin', '/org/kde/KWin/ScreenShot2')
            interface = dbus.Interface(obj, 'org.kde.KWin.ScreenShot2')
            self.logger.info(f"Got KWin ScreenShot2 interface in {time.time() - obj_start:.3f}s")
            
            # Use a temporary file to receive data to avoid pipe deadlocks on large images
            with tempfile.TemporaryFile() as tf:
                fd = tf.fileno()
                dbus_fd = dbus.types.UnixFd(fd)
                
                # Options
                options = {'native-resolution': True}
                
                self.logger.info(f"Requesting screenshot ({method_name})...")
                capture_start = time.time()
                
                # Prepare arguments based on method
                if method_name == 'CaptureWorkspace':
                    metadata = interface.CaptureWorkspace(options, dbus_fd)
                elif method_name == 'CaptureScreen':
                    screen_name = args[0] if args else ''
                    metadata = interface.CaptureScreen(screen_name, options, dbus_fd)
                else:
                    self.logger.error(f"Unknown method: {method_name}")
                    return None
                
                capture_time = time.time() - capture_start
                self.logger.info(f"Screenshot captured in {capture_time:.3f}s")
                self.logger.info(f"Metadata: {metadata}")
                
                # Read the data from the temporary file
                read_start = time.time()
                tf.seek(0)
                data = tf.read()
                read_time = time.time() - read_start
                self.logger.info(f"Read {len(data)} bytes from temporary file in {read_time:.3f}s")
                
                if not data:
                    self.logger.warning("Received empty data.")
                    return None

                width = int(metadata.get('width', 0))
                height = int(metadata.get('height', 0))
                stride = int(metadata.get('stride', 0))
                fmt = int(metadata.get('format', 0))

                self.logger.info(f"Image info: {width}x{height}, stride={stride}, format={fmt}")

                # KWin ScreenShot2 typically returns BGRA data (format 6 or 5)
                if width > 0 and height > 0:
                    bpp = stride // width
                    if bpp == 4:
                        # Likely BGRA
                        processing_start = time.time()
                        mode = "RGBA"
                        raw_mode = "BGRA"
                        image = Image.frombytes(mode, (width, height), data, "raw", raw_mode, stride)
                        
                        # Convert PIL Image to QPixmap for compatibility
                        width, height = image.size
                        ptr = image.tobytes()
                        qimage = QImage(ptr, width, height, QImage.Format.Format_RGBA8888)
                        pixmap = QPixmap.fromImage(qimage)
                        
                        total_time = time.time() - start_time
                        self.logger.info(f"Processed image in {time.time() - processing_start:.3f}s")
                        self.logger.info(f"Total {method_name} capture took {total_time:.3f}s")
                        return pixmap
                    else:
                        # Fallback or other formats
                        self.logger.warning(f"Unsupported bytes per pixel: {bpp}. Saving raw data dump.")
                        with open(f"screenshot_{method_name}.raw", "wb") as f:
                            f.write(data)
                        return None
                else:
                    self.logger.error("Invalid dimensions in metadata.")
                    return None

        except dbus.DBusException as e:
            self.logger.error(f"DBus Error: {e}")
            return None
        except Exception as e:
            self.logger.error(f"Error: {e}")
            import traceback
            self.logger.error(traceback.format_exc())
            return None

    def capture_workspace(self):
        return self._capture_internal('CaptureWorkspace')
    
    def capture_screen(self, screen_name):
        return self._capture_internal('CaptureScreen', screen_name)
