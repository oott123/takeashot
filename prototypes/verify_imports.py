
try:
    import PyQt6
    from PyQt6.QtCore import Qt
    print("PyQt6 found")
except ImportError:
    print("PyQt6 NOT found")
    exit(1)

try:
    import dbus_manager
    print("dbus_manager imported")
except Exception as e:
    print(f"Error importing dbus_manager: {e}")

try:
    import screenshot_backend
    print("screenshot_backend imported")
except Exception as e:
    print(f"Error importing screenshot_backend: {e}")

try:
    import window_lister
    print("window_lister imported")
except Exception as e:
    print(f"Error importing window_lister: {e}")

try:
    import input_monitor
    print("input_monitor imported")
except Exception as e:
    print(f"Error importing input_monitor: {e}")

try:
    import annotations.manager
    print("annotations.manager imported")
except Exception as e:
    print(f"Error importing annotations.manager: {e}")

try:
    import snipping_widget
    print("snipping_widget imported")
except Exception as e:
    print(f"Error importing snipping_widget: {e}")

print("Verification complete")
