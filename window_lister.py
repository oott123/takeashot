import dbus
import dbus.mainloop.pyqt5

# 在导入dbus.service之前设置DBus主循环
from dbus.mainloop.pyqt5 import DBusQtMainLoop
DBusQtMainLoop(set_as_default=True)

import tempfile
import os
import json
import time
from PyQt5.QtCore import QObject, pyqtSignal, QTimer
import dbus.service


# Create a metaclass that inherits from both parent metaclasses to resolve the conflict
class WindowListReceiverMeta(type(QObject), type(dbus.service.Object)):
    pass


class WindowListReceiver(QObject, dbus.service.Object, metaclass=WindowListReceiverMeta):
    """DBus接收器，接收KWin脚本返回的窗口数据"""
    
    windows_received = pyqtSignal(list)
    
    def __init__(self, bus, path):
        QObject.__init__(self)
        dbus.service.Object.__init__(self, bus, path)
        self.data = None
    
    @dbus.service.method("com.takeashot.Receiver", in_signature='s')
    def receive(self, json_str):
        """接收KWin脚本返回的窗口数据"""
        try:
            self.data = json.loads(json_str)
            self.windows_received.emit(self.data)
        except json.JSONDecodeError as e:
            print(f"Failed to parse window data: {e}")
            self.windows_received.emit([])
        return "OK"


class WindowLister(QObject):
    """窗口列表获取器，异步获取KWin窗口列表"""
    
    windows_ready = pyqtSignal(list)
    
    def __init__(self):
        super().__init__()
        self.receiver = None
        self.receiver_path = None
        self.timeout_timer = None
        self.script_id = None
        self.temp_file = None
    
    def get_windows_async(self):
        """异步获取窗口列表，完成后通过windows_ready信号返回数据"""
        try:
            # 获取进程ID用于DBus服务命名（添加前缀避免纯数字组件）
            pid = os.getpid()
            timestamp = int(time.time() * 1000)
            receiver_name = f"com.takeashot.screenshot.pid_{pid}"
            receiver_path = f"/Receiver_{timestamp}"
            
            # 注册DBus接收器，显式传递mainloop参数
            from dbus.mainloop.pyqt5 import DBusQtMainLoop
            bus = dbus.SessionBus(mainloop=DBusQtMainLoop())
            
            bus_name = dbus.service.BusName(receiver_name, bus)
            self.receiver = WindowListReceiver(bus, receiver_path)
            self.receiver_path = receiver_path
            self.receiver.windows_received.connect(self._on_windows_received)
            
            # 生成KWin脚本
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
callDBus("{receiver_name}", "{receiver_path}", "com.takeashot.Receiver", "receive", JSON.stringify(res));
"""
            
            # 写入临时文件
            with tempfile.NamedTemporaryFile(mode='w', suffix='.js', delete=False) as f:
                f.write(js_code)
                self.temp_file = f.name
            
            # 加载并运行KWin脚本
            obj = bus.get_object('org.kde.KWin', '/Scripting')
            iface = dbus.Interface(obj, 'org.kde.kwin.Scripting')
            
            plugin_name = f"takeashot_{int(time.time() * 1000)}"
            self.script_id = iface.loadScript(self.temp_file, plugin_name, signature='ss')
            
            script_path = f"/Scripting/Script{self.script_id}"
            script_obj = bus.get_object('org.kde.KWin', script_path)
            script_iface = dbus.Interface(script_obj, 'org.kde.kwin.Script')
            
            # 运行脚本
            script_iface.run()
            
            # 设置超时（2秒）
            self.timeout_timer = QTimer()
            self.timeout_timer.setSingleShot(True)
            self.timeout_timer.timeout.connect(self._on_timeout)
            self.timeout_timer.start(2000)
            
        except Exception as e:
            print(f"Failed to start window list retrieval: {e}")
            self.windows_ready.emit([])
            self._cleanup()
    
    def _on_windows_received(self, windows):
        """窗口数据接收完成"""
        if self.timeout_timer:
            self.timeout_timer.stop()
        
        self.windows_ready.emit(windows if windows else [])
        self._cleanup()
    
    def _on_timeout(self):
        """获取超时"""
        print("Window list retrieval timed out")
        self.windows_ready.emit([])
        self._cleanup()
    
    def _cleanup(self):
        """清理资源"""
        # 停止超时计时器
        if self.timeout_timer:
            self.timeout_timer.stop()
            self.timeout_timer = None
        
        # 卸载KWin脚本
        if self.script_id is not None:
            try:
                bus = dbus.SessionBus()
                obj = bus.get_object('org.kde.KWin', '/Scripting')
                iface = dbus.Interface(obj, 'org.kde.kwin.Scripting')
                iface.unloadScript(f"takeashot_{self.script_id}")
            except:
                pass
            self.script_id = None
        
        # 删除临时文件
        if self.temp_file and os.path.exists(self.temp_file):
            try:
                os.remove(self.temp_file)
            except:
                pass
            self.temp_file = None
        
        # 移除DBus接收器对象
        if self.receiver:
            try:
                self.receiver.remove_from_connection()
            except:
                pass
            self.receiver = None
            self.receiver_path = None