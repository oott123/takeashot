#!/usr/bin/env python3
import dbus
import os
import time
import tempfile

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

            with open(output_file, "wb") as f:
                f.write(data)
            
            print(f"Saved to {output_file} ({len(data)} bytes).")

    except dbus.DBusException as e:
        print(f"DBus Error: {e}")
    except Exception as e:
        print(f"Error: {e}")

if __name__ == "__main__":
    capture_screenshot()
