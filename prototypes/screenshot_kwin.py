#!/usr/bin/env python3
import dbus
import tempfile
from PIL import Image

def capture_screenshot(output_file="screenshot.png"):
    """
    Captures a screenshot using org.kde.KWin.ScreenShot2 DBus interface.
    Uses CaptureActiveScreen for simplicity.
    """
    try:
        # Connect to Session Bus
        bus = dbus.SessionBus()
        
        # Get the KWin Screenshot2 object
        obj = bus.get_object('org.kde.KWin', '/org/kde/KWin/ScreenShot2')
        interface = dbus.Interface(obj, 'org.kde.KWin.ScreenShot2')
        
        # Use a temporary file to receive data to avoid pipe deadlocks on large images
        # because the DBus call is blocking and KWin might fill the pipe buffer.
        with tempfile.TemporaryFile() as tf:
            fd = tf.fileno()
            dbus_fd = dbus.types.UnixFd(fd)
            
            # Options (empty for defaults)
            options = {}
            
            print("Requesting screenshot (CaptureActiveScreen)...")
            # Call CaptureActiveScreen(options, pipe) -> results
            # Note: The 'pipe' argument is just a file descriptor to write to.
            metadata = interface.CaptureActiveScreen(options, dbus_fd)
            
            print("Screenshot captured.")
            print("Metadata:", metadata)
            
            # Read the data from the temporary file
            tf.seek(0)
            data = tf.read()
            
            if not data:
                print("Warning: Received empty data.")
                return

            width = int(metadata.get('width', 0))
            height = int(metadata.get('height', 0))
            stride = int(metadata.get('stride', 0))
            fmt = int(metadata.get('format', 0))

            print(f"Image info: {width}x{height}, stride={stride}, format={fmt}")

            # KWin ScreenShot2 typically returns BGRA data (format 6 or 5)
            # We construct the image using PIL
            # stride / width gives bytes per pixel.
            if width > 0 and height > 0:
                bpp = stride // width
                if bpp == 4:
                    # Likely BGRA
                    mode = "RGBA"
                    raw_mode = "BGRA"
                    image = Image.frombytes(mode, (width, height), data, "raw", raw_mode, stride)
                    image.save(output_file)
                    print(f"Saved valid PNG to {output_file}")
                else:
                    # Fallback or other formats
                    print(f"Unsupported bytes per pixel: {bpp}. Saving raw data dump.")
                    with open(output_file + ".raw", "wb") as f:
                        f.write(data)
            else:
                print("Invalid dimensions in metadata.")


    except dbus.DBusException as e:
        print(f"DBus Error: {e}")
    except Exception as e:
        print(f"Error: {e}")

if __name__ == "__main__":
    capture_screenshot()
