import evdev
import select
from PyQt5.QtCore import QThread, pyqtSignal

class GlobalInputMonitor(QThread):
    pause_key_pressed = pyqtSignal()

    def run(self):
        try:
            devices = [evdev.InputDevice(path) for path in evdev.list_devices()]
        except Exception as e:
            print(f"GlobalInputMonitor: Failed to list devices: {e}")
            return

        # Filter for keyboards. This might need refinement.
        keyboards = [d for d in devices if "keyboard" in d.name.lower()]
        
        if not keyboards:
             print("GlobalInputMonitor: No keyboard devices found.")
             return
             
        device_map = {dev.fd: dev for dev in keyboards}
        print(f"GlobalInputMonitor: Monitoring {len(keyboards)} keyboards.")
        
        while not self.isInterruptionRequested():
            # Use a timeout to allow checking isInterruptionRequested
            try:
                r, _, _ = select.select(device_map.keys(), [], [], 0.5)
            except OSError:
                # Can happen if a device disconnects during select?
                break

            for fd in r:
                if fd not in device_map:
                    continue
                dev = device_map[fd]
                try:
                    for event in dev.read():
                        if event.type == evdev.ecodes.EV_KEY and \
                           event.code == evdev.ecodes.KEY_PAUSE and \
                           event.value == 1: # Key down
                            self.pause_key_pressed.emit()
                except OSError:
                    # Device disconnected
                    del device_map[fd]
