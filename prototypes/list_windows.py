import dbus
import dbus.service
import dbus.mainloop.pyqt5
from PyQt5.QtCore import QCoreApplication, QTimer
import os
import tempfile
import uuid
import json
import sys

class WindowDataReceiver(dbus.service.Object):
    def __init__(self, bus, path):
        super().__init__(bus, path)
        self.data = None

    @dbus.service.method("com.takeashot.Receiver", in_signature='s')
    def receive(self, json_str):
        self.data = json.loads(json_str)
        QCoreApplication.instance().quit()
        return "OK"

def get_all_windows():
    # Use PyQt5 main loop for DBus
    app = QCoreApplication(sys.argv)
    dbus.mainloop.pyqt5.DBusQtMainLoop(set_as_default=True)
    
    bus = dbus.SessionBus()
    receiver_name = f"com.takeashot.receiver_{uuid.uuid4().hex[:8]}"
    bus_name = dbus.service.BusName(receiver_name, bus)
    receiver = WindowDataReceiver(bus, "/Receiver")
    
    # Connect to KWin
    try:
        obj = bus.get_object('org.kde.KWin', '/Scripting')
        iface = dbus.Interface(obj, 'org.kde.kwin.Scripting')
    except dbus.DBusException as e:
        print(f"Error connecting to KWin: {e}")
        return

    js_code = f"""
    var list = workspace.windowList ? workspace.windowList() : workspace.clientList();
    var res = [];
    for (var i = 0; i < list.length; i++) {{
        var w = list[i];
        if (w.normalWindow && !w.minimized) {{
            var geom = w.frameGeometry ? w.frameGeometry : {{ x: w.x, y: w.y, width: w.width, height: w.height }};
            res.push({{
                title: String(w.caption || w.title || ""),
                resourceClass: String(w.resourceClass || ""),
                x: geom.x,
                y: geom.y,
                width: geom.width,
                height: geom.height
            }});
        }}
    }}
    callDBus("{receiver_name}", "/Receiver", "com.takeashot.Receiver", "receive", JSON.stringify(res));
    """

    with tempfile.NamedTemporaryFile(mode='w', suffix='.js', delete=False) as f:
        f.write(js_code)
        temp_path = f.name

    plugin_name = f"callback_{uuid.uuid4().hex[:8]}"
    try:
        script_id = iface.loadScript(temp_path, plugin_name, signature='ss')
        script_path = f"/Scripting/Script{script_id}"
        script_obj = bus.get_object('org.kde.KWin', script_path)
        script_iface = dbus.Interface(script_obj, 'org.kde.kwin.Script')
        
        # Timeout after 2 seconds if no response
        QTimer.singleShot(2000, app.quit)
        
        script_iface.run()
        app.exec_()
        
        return receiver.data
    finally:
        try:
            iface.unloadScript(plugin_name)
        except:
            pass
        if os.path.exists(temp_path):
            os.remove(temp_path)

if __name__ == "__main__":
    windows = get_all_windows()
    if windows:
        print(f"{'Title':<40} | {'Class':<20} | {'Position/Size':<20}")
        print("-" * 85)
        for w in windows:
            pos_size = f"{w['x']},{w['y']} {w['width']}x{w['height']}"
            print(f"{w['title'][:40]:<40} | {w['resourceClass'][:20]:<20} | {pos_size}")
    else:
        print("Failed to get windows or timeout.")
