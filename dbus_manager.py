from PyQt6.QtCore import QObject, pyqtSignal, pyqtSlot, pyqtClassInfo
from PyQt6.QtDBus import QDBusConnection, QDBusAbstractAdaptor, QDBusMessage, QDBusInterface
import json

@pyqtClassInfo("D-Bus Interface", "com.takeashot.Service")
@pyqtClassInfo("D-Bus Introspection", """
  <interface name="com.takeashot.Service">
    <method name="activate"/>
    <method name="receive_window_data">
      <arg direction="in" type="s" name="json_str"/>
    </method>
  </interface>
""")
class DbusAdaptor(QDBusAbstractAdaptor):
    """
    DBus Adaptor to handle incoming DBus calls.
    Proxies calls to the DbusManager via signals.
    """

    def __init__(self, parent):
        super().__init__(parent)
        self.manager = parent
        self.setAutoRelaySignals(True)

    @pyqtSlot()
    def activate(self):
        """DBus method to trigger activation (screenshot) in this instance."""
        print("Activation requested via DBus")
        self.manager.activation_requested.emit()

    @pyqtSlot(str)
    def receive_window_data(self, json_str):
        """DBus method to receive window data from KWin script."""
        try:
            data = json.loads(json_str)
            self.manager.windows_received.emit(data)
        except json.JSONDecodeError as e:
            print(f"Failed to parse window data: {e}")
            self.manager.windows_received.emit([])
            

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
        self.adaptor = DbusAdaptor(self)
        
    def register_service(self):
        """
        Registers the DBus service. 
        Returns True if successful (acquired name), False otherwise.
        """
        try:
            connection = QDBusConnection.sessionBus()
            if not connection.isConnected():
                print("Cannot connect to the D-Bus session bus.")
                return False

            if not connection.registerObject(self.OBJECT_PATH, self):
                print(f"Failed to register object at {self.OBJECT_PATH}")
                return False
                
            # Request name, do not replace existing owner, do not queue
            # QDBusConnection.registerService handles the low-level name requesting
            if connection.registerService(self.SERVICE_NAME):
                print(f"DBus service registered: {self.SERVICE_NAME}")
                return True
            else:
                # We failed to register the service, likely because another instance holds it
                # Make sure we unregister the object so we don't handle calls unintentionally if we stay alive (though we exit)
                connection.unregisterObject(self.OBJECT_PATH)
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
            # Create an interface to the existing service
            iface = QDBusInterface(self.SERVICE_NAME, self.OBJECT_PATH, self.INTERFACE_NAME, QDBusConnection.sessionBus())
            if iface.isValid():
                iface.call("activate")
                return True
            else:
                print("Invalid DBus interface to existing instance.")
                return False
        except Exception as e:
            print(f"Failed to activate existing instance: {e}")
            return False
