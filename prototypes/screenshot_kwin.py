#!/usr/bin/env python3
import dbus
import tempfile
import time
import logging
from PIL import Image

logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(levelname)s - %(message)s')
logger = logging.getLogger(__name__)

def capture_screenshot(output_file="screenshot.png"):
    """
    Captures a screenshot using org.kde.KWin.ScreenShot2 DBus interface.
    Uses CaptureActiveScreen for simplicity.
    """
    start_time = time.time()
    logger.info(f"Starting screenshot capture to {output_file}")
    
    try:
        # Connect to Session Bus
        bus_start = time.time()
        bus = dbus.SessionBus()
        logger.info(f"Connected to DBus session bus in {time.time() - bus_start:.3f}s")
        
        # Get the KWin Screenshot2 object
        obj_start = time.time()
        obj = bus.get_object('org.kde.KWin', '/org/kde/KWin/ScreenShot2')
        interface = dbus.Interface(obj, 'org.kde.KWin.ScreenShot2')
        logger.info(f"Got KWin ScreenShot2 interface in {time.time() - obj_start:.3f}s")
        
        # Use a temporary file to receive data to avoid pipe deadlocks on large images
        # because the DBus call is blocking and KWin might fill the pipe buffer.
        with tempfile.TemporaryFile() as tf:
            fd = tf.fileno()
            dbus_fd = dbus.types.UnixFd(fd)
            
            # Options (empty for defaults)
            options = {}
            
            logger.info("Requesting screenshot (CaptureActiveScreen)...")
            capture_start = time.time()
            # Call CaptureActiveScreen(options, pipe) -> results
            # Note: The 'pipe' argument is just a file descriptor to write to.
            metadata = interface.CaptureActiveScreen(options, dbus_fd)
            capture_time = time.time() - capture_start
            
            logger.info(f"Screenshot captured in {capture_time:.3f}s")
            logger.info(f"Metadata: {metadata}")
            
            # Read the data from the temporary file
            read_start = time.time()
            tf.seek(0)
            data = tf.read()
            read_time = time.time() - read_start
            logger.info(f"Read {len(data)} bytes from temporary file in {read_time:.3f}s")
            
            if not data:
                logger.warning("Received empty data.")
                return

            width = int(metadata.get('width', 0))
            height = int(metadata.get('height', 0))
            stride = int(metadata.get('stride', 0))
            fmt = int(metadata.get('format', 0))

            logger.info(f"Image info: {width}x{height}, stride={stride}, format={fmt}")

            # KWin ScreenShot2 typically returns BGRA data (format 6 or 5)
            # We construct the image using PIL
            # stride / width gives bytes per pixel.
            if width > 0 and height > 0:
                bpp = stride // width
                if bpp == 4:
                    # Likely BGRA
                    processing_start = time.time()
                    mode = "RGBA"
                    raw_mode = "BGRA"
                    image = Image.frombytes(mode, (width, height), data, "raw", raw_mode, stride)
                    image.save(output_file)
                    processing_time = time.time() - processing_start
                    total_time = time.time() - start_time
                    logger.info(f"Processed and saved PNG to {output_file} in {processing_time:.3f}s")
                    logger.info(f"Total screenshot process completed in {total_time:.3f}s")
                else:
                    # Fallback or other formats
                    logger.warning(f"Unsupported bytes per pixel: {bpp}. Saving raw data dump.")
                    with open(output_file + ".raw", "wb") as f:
                        f.write(data)
            else:
                logger.error("Invalid dimensions in metadata.")


    except dbus.DBusException as e:
        logger.error(f"DBus Error: {e}")
    except Exception as e:
        logger.error(f"Error: {e}")

if __name__ == "__main__":
    capture_screenshot()
