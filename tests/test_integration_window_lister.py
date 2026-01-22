import pytest
from PyQt6.QtCore import QCoreApplication
from PyQt6.QtDBus import QDBusConnection, QDBusInterface
import sys
import os
import time

# Add project root to path
sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), '..')))

from window_lister import WindowLister
from dbus_manager import DbusManager

# Skip if no DBus session (though in this env we expect one, or at least we try)
# But here we assume we can test IPC between objects in the same process via SessionBus
# which QtDBus supports.

def test_integration_dbus_communication(qtbot):
    """
    Test that WindowLister can receive signals from DbusManager via actual DBus.
    This doesn't test KWin interaction, but tests the internal wiring + DBus transport.
    """
    dbus_manager = DbusManager()
    if not dbus_manager.register_service():
        pytest.skip("Could not register DBus service, maybe another instance is running?")
    
    window_lister = WindowLister(dbus_manager)
    
    received_windows = []
    def on_windows_ready(windows):
        received_windows.append(windows)
        
    window_lister.windows_ready.connect(on_windows_ready)
    
    # We want to manually trigger 'windows_received' on DbusManager 
    # BUT WindowLister only connects to it inside get_windows_async()
    # So we need to start get_windows_async, but we don't want it to actually call KWin if likely to fail.
    # However, the user wants "REAL" integration.
    # If we run get_windows_async, it tries to load KWin script. 
    # If KWin is not there, it errors out or prints invalid interface.
    
    start_time = time.time()
    
    # Let's try to simulate the data coming from "outside" (or even inside via Adaptor)
    # 1. Start listening
    # We compromise: We call get_windows_async(); if it fails to load script, it might clean up.
    # BUT we can check if it hooked up the signal.
    
    # To test pure signal delivery without KWin:
    # default behavior of get_windows_async is complex. 
    # Let's verify DbusManager.windows_received -> WindowLister logic directly first?
    # No, we need to test the whole flow.
    
    # If we are on a system without KWin scripting, get_windows_async might fail early.
    # Let's patch QDBusInterface in this integration test ONLY for the KWin part
    # BUT keep the DbusManager part real.
    
    # Actually, the user asked for REAL DBus.
    # So we will try to use DbusAdaptor to inject data.
    
    # Manually connect for this test since we might not call get_windows_async if we want to bypass KWin
    # or we mock KWin part but use real DBus for the return path.
    
    # Let's rely on the fact that DbusAdaptor is listening on DBus.
    # We will invoke the DBus method 'receive_window_data' on the bus, 
    # which should trigger DbusManager signal, which WindowLister is connected to.
    
    # 1. Manually connect WindowLister to DbusManager (simulating what get_windows_async does)
    window_lister.dbus_manager.windows_received.connect(window_lister._on_windows_received)
    window_lister.is_connected = True
    
    # 2. Call the DBus method via QDBusInterface (client side)
    # This simulates the KWin script calling back into our application
    client_iface = QDBusInterface(
        DbusManager.SERVICE_NAME,
        DbusManager.OBJECT_PATH,
        DbusManager.INTERFACE_NAME,
        QDBusConnection.sessionBus()
    )
    
    assert client_iface.isValid(), "Client interface should be valid"
    
    test_json = '[{"title": "Real DBus Window", "x": 10, "y": 10, "width": 200, "height": 200}]'
    
    # 3. Call receive_window_data asynchronously to avoid blocking the event loop
    # Since we are in the same process/thread, a blocking call would deadlock 
    # (waiting for reply while main loop is blocked).
    with qtbot.waitSignal(window_lister.windows_ready, timeout=3000) as blocker:
        call = client_iface.asyncCall("receive_window_data", test_json)
        # We don't necessarily need to wait for the reply, just the signal emission
        
    assert len(blocker.args[0]) == 1
    assert blocker.args[0][0]['title'] == "Real DBus Window"
    
    # Clean up
    connection = QDBusConnection.sessionBus()
    connection.unregisterService(DbusManager.SERVICE_NAME)
    connection.unregisterObject(DbusManager.OBJECT_PATH)

