import tempfile
import os
import json
import time
from PyQt6.QtCore import QObject, pyqtSignal, QTimer
from PyQt6.QtDBus import QDBusInterface, QDBusConnection, QDBusMessage

class WindowLister(QObject):
    """窗口列表获取器，异步获取KWin窗口列表"""
    
    windows_ready = pyqtSignal(list)
    
    def __init__(self, dbus_manager):
        super().__init__()
        self.dbus_manager = dbus_manager
        self.timeout_timer = None
        self.script_id = None
        self.plugin_name = None
        self.temp_file = None
        self.is_connected = False
    
    def get_windows_async(self):
        """异步获取窗口列表，完成后通过windows_ready信号返回数据"""
        try:
            # Connect to dbus manager signal
            if not self.is_connected:
                self.dbus_manager.windows_received.connect(self._on_windows_received)
                self.is_connected = True

            # Constants from DbusManager
            service_name = self.dbus_manager.SERVICE_NAME
            object_path = self.dbus_manager.OBJECT_PATH
            interface_name = self.dbus_manager.INTERFACE_NAME
            method_name = "receive_window_data"
            
            # 生成KWin脚本
            js_code = f"""
var list = workspace.stackingOrder ? workspace.stackingOrder : (workspace.windowList ? workspace.windowList() : workspace.clientList());
var res = [];
for (var i = list.length - 1; i >= 0; i--) {{
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
try {{
    callDBus("{service_name}", "{object_path}", "{interface_name}", "{method_name}", JSON.stringify(res));
}} catch (e) {{
    print("Error calling DBus from KWin script: " + e);
}}
"""
            
            # 写入临时文件
            with tempfile.NamedTemporaryFile(mode='w', suffix='.js', delete=False) as f:
                f.write(js_code)
                self.temp_file = f.name
            
            # 加载并运行KWin脚本 using QDBusInterface
            scripting_iface = QDBusInterface(
                'org.kde.KWin', 
                '/Scripting', 
                'org.kde.kwin.Scripting', 
                QDBusConnection.sessionBus()
            )
            
            if not scripting_iface.isValid():
                print(f"Error: Invalid DBus interface for KWin Scripting: {scripting_iface.lastError().message()}")
                self.windows_ready.emit([])
                self._cleanup()
                return

            self.plugin_name = f"takeashot_{int(time.time() * 1000)}"
            # loadScript(path, pluginName) returns int scriptId
            reply = scripting_iface.call('loadScript', self.temp_file, self.plugin_name)
            
            if reply.type() == QDBusMessage.MessageType.ErrorMessage:
                print(f"Failed to load KWin script: {reply.errorMessage()}")
                self.windows_ready.emit([])
                self._cleanup()
                return
                
            self.script_id = int(reply.arguments()[0])
            
            # Run the script
            script_path = f"/Scripting/Script{self.script_id}"
            script_iface = QDBusInterface(
                'org.kde.KWin', 
                script_path, 
                'org.kde.kwin.Script', 
                QDBusConnection.sessionBus()
            )
            
            if script_iface.isValid():
                script_iface.call('run')
            else:
                print("Failed to get script interface")
            
            # 设置超时（5秒）
            self.timeout_timer = QTimer()
            self.timeout_timer.setSingleShot(True)
            self.timeout_timer.timeout.connect(self._on_timeout)
            self.timeout_timer.start(5000)
            
        except Exception as e:
            print(f"Failed to start window list retrieval: {e}")
            import traceback
            traceback.print_exc()
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
        
        # Disconnect from signal to avoid duplicate calls if reused or memory leaks
        if self.is_connected:
            try:
                self.dbus_manager.windows_received.disconnect(self._on_windows_received)
            except:
                pass
            self.is_connected = False
        
        # 卸载KWin脚本
        if self.script_id is not None and self.plugin_name:
            try:
                scripting_iface = QDBusInterface(
                    'org.kde.KWin', 
                    '/Scripting', 
                    'org.kde.kwin.Scripting', 
                    QDBusConnection.sessionBus()
                )
                if scripting_iface.isValid():
                    scripting_iface.call('unloadScript', self.plugin_name)
            except Exception as e:
                print(f"Error unloading script: {e}")
            self.script_id = None
            self.plugin_name = None
        
        # 删除临时文件
        if self.temp_file and os.path.exists(self.temp_file):
            try:
                os.remove(self.temp_file)
            except:
                pass
            self.temp_file = None