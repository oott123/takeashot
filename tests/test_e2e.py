
import pytest
from PyQt6.QtCore import Qt, QPoint, QRect, QTimer
from PyQt6.QtWidgets import QApplication

def test_app_startup(app, qtbot):
    """Test that the application starts and tray icon is visible."""
    # assert app.tray_icon.isVisible() 
    # SystemTrayIcon visibility is tricky in headless/test envs, often returns False.
    # Instead check if it exists and has an icon.
    assert app.tray_icon is not None
    assert not app.tray_icon.icon().isNull()

def test_capture_mode_initialization(app, qtbot):
    """Test entering capture mode creates SnippingWindows."""
    # Ensure no snippers initially
    app.close_all_snippers()
    assert len(app.snippers) == 0
    
    # Trigger capture
    # We manually trigger the start_capture directly
    app.start_capture()
        
    # Check snippers created (assuming at least one screen)
    assert len(app.snippers) > 0
    
    # Check snipper properties
    snipper = app.snippers[0]
    assert snipper.isVisible()
    assert snipper.windowState() & Qt.WindowState.WindowFullScreen

def test_selection_logic(app, qtbot):
    """Test dragging to create a selection."""
    app.start_capture()
    assert len(app.snippers) > 0
    snipper = app.snippers[0]
    widget = snipper.snipping_widget
    
    # Simulate drag
    # Local coordinates on the widget
    start_pos = QPoint(100, 100)
    end_pos = QPoint(300, 200)
    
    # Map to global for the controller (ScreenshotApp handles global pos)
    # The widget uses event.globalPosition()
    # SnippingWidget is at (0,0) of the window usually.
    # We need to simulate events relative to the widget but the code uses global pos.
    # qtbot.mousePress sends events to the widget.
    
    # We need to ensure the widget is shown and mapped
    qtbot.addWidget(snipper)
    with qtbot.waitExposed(snipper):
        pass
    
    # Calculate global positions
    # snipper.mapToGlobal(pos) might not be accurate if window manager manages it, 
    # but for frameless fullscreen it should be screen coords.
    global_start = snipper.mapToGlobal(start_pos)
    global_end = snipper.mapToGlobal(end_pos)
    
    # 1. Mouse Press
    qtbot.mouseMove(widget, start_pos)
    qtbot.mousePress(widget, Qt.MouseButton.LeftButton, pos=start_pos)
    
    # Check state
    assert app.is_selecting
    assert app.click_start_pos == global_start
    
    # 2. Mouse Move (Drag)
    qtbot.mouseMove(widget, end_pos)
    
    # Check selection rect
    expected_rect = QRect(global_start, global_end).normalized()
    assert app.selection_rect == expected_rect
    
    # 3. Mouse Release
    qtbot.mouseRelease(widget, Qt.MouseButton.LeftButton, pos=end_pos)
    
    assert not app.is_selecting
    assert app.selection_rect == expected_rect

def test_cancel_on_escape(app, qtbot):
    """Test that pressing Escape closes the capture session."""
    app.start_capture()
    assert len(app.snippers) > 0
    snipper = app.snippers[0]
    qtbot.addWidget(snipper)
    with qtbot.waitExposed(snipper):
        pass
    
    # Press Escape
    qtbot.keyPress(snipper, Qt.Key.Key_Escape)
    
    # Wait for close
    def check_closed():
        return len(app.snippers) == 0
        
    qtbot.waitUntil(check_closed, timeout=1000)
    assert len(app.snippers) == 0
