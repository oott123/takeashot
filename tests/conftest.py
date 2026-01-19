
import pytest
from unittest.mock import MagicMock
import sys
import os

# Ensure the project root is in sys.path
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '..')))

from main import ScreenshotApp

@pytest.fixture
def mock_dbus_manager(monkeypatch):
    """Mock the DbusManager to avoid real DBus interactions during tests."""
    MockDbusManager = MagicMock()
    # Mock the instance methods
    instance = MockDbusManager.return_value
    instance.register_service.return_value = True  # Pretend we are the primary instance
    
    # Mock the signal (activation_requested)
    # Ideally DbusManager has a real signal, but since we mock the whole class,
    # we need to simulate the signal object if it's accessed.
    # However, in main.py it accesses self.dbus_manager.activation_requested.connect(...)
    # So we need instance.activation_requested to have a connect method.
    instance.activation_requested = MagicMock()
    instance.activation_requested.connect = MagicMock()
    
    # Patch the class in main.py
    monkeypatch.setattr("main.DbusManager", MockDbusManager)
    return instance

@pytest.fixture
def app(qtbot, mock_dbus_manager):
    """Fixture to create the ScreenshotApp instance."""
    # The QApplication is handled by pytest-qt (via qtbot implicit dependency or qapp fixture)
    # But ScreenshotApp creates its own QApplication if not present.
    # main.py was refactored to use QApplication.instance() if available.
    
    screenshot_app = ScreenshotApp()
    
    # We might want to disable the global input monitor for tests to avoid seizing inputs
    # In main.py: self.input_monitor = GlobalInputMonitor(); self.input_monitor.start()
    # Let's stop it if it's running
    if hasattr(screenshot_app, 'input_monitor'):
        # Just to be safe, though mocking it effectively would be better
        # For now, let's assume it doesn't block unless we interact with it
        pass

    yield screenshot_app
    
    # Teardown
    screenshot_app.close_all_snippers()
    # Check if we need to quit app? pytest-qt handles qapp exit usually.
