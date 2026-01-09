import dbus
import dbus.service
import dbus.mainloop.pyqt5
from PyQt5.QtCore import QObject, pyqtSignal
import json

class DbusAdaptor(dbus.service.Object):
    """
    DBus Adaptor to handle incoming DBus calls.
    Proxies calls to the DbusManager via signals.
    """
    def __init__(self, manager, bus, path):
        self.manager = manager
        super().__init__(bus, path)

    @dbus.service.method("com.takeashot.Service", in_signature='', out_signature='')
    def activate(self):
        """DBus method to trigger activation (screenshot) in this instance."""
        print("Activation requested via DBus")
        self.manager.activation_requested.emit()

    @dbus.service.method("com.takeashot.Service", in_signature='s', out_signature='s')
    def receive_window_data(self, json_str):
        """DBus method to receive window data from KWin script."""
        try:
            data = json.loads(json_str)
            self.manager.windows_received.emit(data)
        except json.JSONDecodeError as e:
            print(f"Failed to parse window data: {e}")
            self.manager.windows_received.emit([])
        return "OK"


class DbusManager(QObject):
    """
    Unified DBus manager for the application.
    Handles service registration and coordinates with DbusAdaptor.
    """
    
    # Signal emitted when activation is requested (e.g. from another instance)
    activation_requested = pyqtSignal()
    
    # Signal emitted when window data is received from KWin script
    windows_received = pyqtSignal(list)

    SERVICE_NAME = "com.takeashot.service"
    OBJECT_PATH = "/com/takeashot/Service"
    INTERFACE_NAME = "com.takeashot.Service"

    def __init__(self):
        super().__init__()
        self.bus = dbus.SessionBus()
        self.bus_name = None
        self.adaptor = None
        
    def register_service(self):
        """
        Registers the DBus service. 
        Returns True if successful (acquired name), False otherwise.
        """
        try:
            # Request name, do not replace existing owner, do not queue
            ret = self.bus.request_name(self.SERVICE_NAME, dbus.bus.NAME_FLAG_DO_NOT_QUEUE)
            
            if ret == dbus.bus.REQUEST_NAME_REPLY_PRIMARY_OWNER:
                # We own the name, register the adaptor (object)
                self.bus_name = dbus.service.BusName(self.SERVICE_NAME, self.bus)
                self.adaptor = DbusAdaptor(self, self.bus, self.OBJECT_PATH)
                print(f"DBus service registered: {self.SERVICE_NAME}")
                return True
            else:
                print(f"DBus service {self.SERVICE_NAME} already exists.")
                return False
        except Exception as e:
            print(f"Failed to register DBus service: {e}")
            return False

    def trigger_activate_on_existing_instance(self):
        """
        Attempts to call the activate method on an existing instance.
        """
        try:
            obj = self.bus.get_object(self.SERVICE_NAME, self.OBJECT_PATH)
            iface = dbus.Interface(obj, self.INTERFACE_NAME)
            iface.activate()
            return True
        except Exception as e:
            print(f"Failed to activate existing instance: {e}")
            return False
