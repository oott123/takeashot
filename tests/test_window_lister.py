import pytest
from PyQt6.QtCore import QObject, QTimer, pyqtSignal
from PyQt6.QtDBus import QDBusMessage
from unittest.mock import MagicMock, patch
import sys
import os

# Add project root to path
sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), '..')))

from window_lister import WindowLister

class MockDbusManager(QObject):
    windows_received = pyqtSignal(list)
    SERVICE_NAME = "com.test.service"
    OBJECT_PATH = "/com/test/Service"
    INTERFACE_NAME = "com.test.Service"

@pytest.fixture
def dbus_manager():
    return MockDbusManager()

@pytest.fixture
def window_lister(dbus_manager):
    lister = WindowLister(dbus_manager)
    return lister

def test_initialization(window_lister):
    assert window_lister.timeout_timer is None
    assert window_lister.script_id is None

@patch('window_lister.QDBusInterface')
@patch('window_lister.QDBusConnection')
def test_get_windows_async_success(mock_connection, mock_interface, window_lister, dbus_manager):
    # Mock scripting interface
    mock_scripting_iface = MagicMock()
    mock_scripting_iface.isValid.return_value = True
    
    # Mock loadScript reply
    mock_reply = MagicMock()
    mock_reply.type.return_value = QDBusMessage.MessageType.ReplyMessage
    mock_reply.arguments.return_value = [123] # script_id
    mock_scripting_iface.call.return_value = mock_reply
    
    # Mock script interface (run)
    mock_script_iface = MagicMock()
    mock_script_iface.isValid.return_value = True
    
    # Configure mock_interface constructor to return different mocks based on args
    def side_effect(*args):
        if args[1] == '/Scripting':
            return mock_scripting_iface
        elif args[1] == '/Scripting/Script123':
            return mock_script_iface
        return MagicMock()
    
    mock_interface.side_effect = side_effect

    # Connect signal
    received_windows = []
    def on_windows_ready(windows):
        received_windows.append(windows)
    window_lister.windows_ready.connect(on_windows_ready)

    # Call method
    window_lister.get_windows_async()

    # Verify script loaded and run
    mock_scripting_iface.call.assert_any_call('loadScript', window_lister.temp_file, window_lister.plugin_name)
    mock_script_iface.call.assert_called_with('run')
    
    # Simulate DBus callback
    test_data = [{'title': 'Test Window', 'x': 0, 'y': 0, 'width': 100, 'height': 100}]
    dbus_manager.windows_received.emit(test_data)
    
    assert len(received_windows) == 1
    assert received_windows[0] == test_data
    # Cleanup should happen
    assert window_lister.script_id is None
    assert window_lister.temp_file is None

@patch('window_lister.QDBusInterface')
@patch('window_lister.QDBusConnection')
def test_get_windows_async_timeout(mock_connection, mock_interface, window_lister, dbus_manager):
    # Setup mocks similar to success case
    mock_scripting_iface = MagicMock()
    mock_scripting_iface.isValid.return_value = True
    mock_reply = MagicMock()
    mock_reply.type.return_value = QDBusMessage.MessageType.ReplyMessage
    mock_reply.arguments.return_value = [123]
    mock_scripting_iface.call.return_value = mock_reply
    mock_script_iface = MagicMock()
    mock_script_iface.isValid.return_value = True
    
    def side_effect(*args):
        if args[1] == '/Scripting':
            return mock_scripting_iface
        elif args[1] == '/Scripting/Script123':
            return mock_script_iface
        return MagicMock()
    mock_interface.side_effect = side_effect

    received_windows = []
    window_lister.windows_ready.connect(lambda w: received_windows.append(w))

    window_lister.get_windows_async()
    
    # Trigger timeout manually
    if window_lister.timeout_timer:
        window_lister.timeout_timer.timeout.emit()
        
    assert len(received_windows) == 1
    assert received_windows[0] == [] # Should be empty list on timeout
