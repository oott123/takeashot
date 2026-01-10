import evdev
import select
import sys

def list_devices():
    devices = [evdev.InputDevice(path) for path in evdev.list_devices()]
    print("Available devices:")
    for device in devices:
        print(f"{device.path}: {device.name} - {device.phys}")
    return devices

def get_keyboard_devices():
    devices = [evdev.InputDevice(path) for path in evdev.list_devices()]
    keyboards = []
    for device in devices:
        if "keyboard" in device.name.lower():
            keyboards.append(device)
    return keyboards

def main():
    print("Listing devices...")
    list_devices()
    
    keyboards = get_keyboard_devices()
    if not keyboards:
        print("No keyboard devices found that match 'keyboard'.")
        # Fallback to monitoring all?
        devices = [evdev.InputDevice(path) for path in evdev.list_devices()]
    else:
        print(f"Found {len(keyboards)} keyboard-like devices.")
        devices = keyboards

    print(f"Monitoring {len(devices)} devices for 'KEY_PAUSE' (code {evdev.ecodes.KEY_PAUSE})...")
    
    # Create a map of file descriptors to devices
    device_map = {dev.fd: dev for dev in devices}
    
    try:
        while True:
            r, w, x = select.select(device_map.keys(), [], [])
            for fd in r:
                dev = device_map[fd]
                for event in dev.read():
                    if event.type == evdev.ecodes.EV_KEY:
                        key_event = evdev.categorize(event)
                        if event.value == 1: # Key down
                            print(f"Key detected: {key_event.keycode} ({event.code}) on {dev.name}")
                            if event.code == evdev.ecodes.KEY_PAUSE:
                                print(">>> PAUSE KEY DETECTED! <<<")
    except KeyboardInterrupt:
        print("Stopping...")

if __name__ == "__main__":
    main()
