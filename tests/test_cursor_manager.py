import pytest
from unittest.mock import MagicMock, PropertyMock
from PyQt6.QtCore import Qt, QRect, QPoint
from cursor_manager import CursorManager


@pytest.fixture
def mock_controller():
    controller = MagicMock()
    controller.selection_rect = QRect(0, 0, 100, 100)
    controller.click_start_pos = QRect(0, 0, 0, 0).topLeft()
    controller.active_handle = None
    controller.is_selecting = False
    return controller


@pytest.fixture
def mock_widget():
    widget = MagicMock()
    return widget


@pytest.fixture
def cursor_manager(mock_controller, mock_widget):
    return CursorManager(mock_controller, mock_widget)


def test_no_selection_sets_cross_cursor(cursor_manager, mock_controller, mock_widget):
    mock_controller.selection_rect = QRect()
    cursor_manager.update_cursor(QPoint(50, 50))
    mock_widget.setCursor.assert_called_once_with(Qt.CursorShape.CrossCursor)


def test_pending_selection_sets_cross_cursor(cursor_manager, mock_controller, mock_widget):
    mock_controller.selection_rect = QRect()
    cursor_manager.update_cursor(QPoint(50, 50))
    mock_widget.setCursor.assert_called_once_with(Qt.CursorShape.CrossCursor)


def test_inside_selection_pointer_tool_sets_size_all(cursor_manager, mock_controller, mock_widget):
    mock_controller.selection_rect = QRect(0, 0, 100, 100)
    annotation_manager = MagicMock()
    annotation_manager.current_tool = 'pointer'
    annotation_manager.selected_item = None
    mock_controller.annotation_manager = annotation_manager
    cursor_manager.update_cursor(QPoint(50, 50))
    mock_widget.setCursor.assert_called_once_with(Qt.CursorShape.SizeAllCursor)


def test_inside_selection_annotation_tool_sets_cross(cursor_manager, mock_controller, mock_widget):
    mock_controller.selection_rect = QRect(0, 0, 100, 100)
    mock_controller.annotation_manager.current_tool = 'pencil'
    cursor_manager.update_cursor(QPoint(50, 50))
    mock_widget.setCursor.assert_called_once_with(Qt.CursorShape.CrossCursor)


def test_outside_top_left_sets_fdiag(cursor_manager, mock_controller, mock_widget):
    mock_controller.selection_rect = QRect(0, 0, 100, 100)
    mock_controller.is_selecting = False
    mock_controller.active_handle = None
    cursor_manager.update_cursor(QPoint(-10, -10))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeFDiagCursor)


def test_outside_top_right_sets_bdiag(cursor_manager, mock_controller, mock_widget):
    mock_controller.selection_rect = QRect(0, 0, 100, 100)
    mock_controller.is_selecting = False
    mock_controller.active_handle = None
    cursor_manager.update_cursor(QPoint(200, -10))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeBDiagCursor)


def test_outside_bottom_left_sets_bdiag(cursor_manager, mock_controller, mock_widget):
    mock_controller.selection_rect = QRect(0, 0, 100, 100)
    mock_controller.is_selecting = False
    mock_controller.active_handle = None
    cursor_manager.update_cursor(QPoint(-10, 200))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeBDiagCursor)


def test_outside_bottom_right_sets_fdiag(cursor_manager, mock_controller, mock_widget):
    mock_controller.selection_rect = QRect(0, 0, 100, 100)
    mock_controller.is_selecting = False
    mock_controller.active_handle = None
    cursor_manager.update_cursor(QPoint(200, 200))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeFDiagCursor)


def test_outside_left_sets_horizontal(cursor_manager, mock_controller, mock_widget):
    mock_controller.selection_rect = QRect(0, 0, 100, 100)
    mock_controller.is_selecting = False
    mock_controller.active_handle = None
    cursor_manager.update_cursor(QPoint(-10, 50))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeHorCursor)


def test_outside_right_sets_horizontal(cursor_manager, mock_controller, mock_widget):
    mock_controller.selection_rect = QRect(0, 0, 100, 100)
    mock_controller.is_selecting = False
    mock_controller.active_handle = None
    cursor_manager.update_cursor(QPoint(200, 50))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeHorCursor)


def test_outside_top_sets_vertical(cursor_manager, mock_controller, mock_widget):
    mock_controller.selection_rect = QRect(0, 0, 100, 100)
    mock_controller.is_selecting = False
    mock_controller.active_handle = None
    cursor_manager.update_cursor(QPoint(50, -10))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeVerCursor)


def test_outside_bottom_sets_vertical(cursor_manager, mock_controller, mock_widget):
    mock_controller.selection_rect = QRect(0, 0, 100, 100)
    mock_controller.is_selecting = False
    mock_controller.active_handle = None
    cursor_manager.update_cursor(QPoint(50, 200))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeVerCursor)


def test_expanding_br_diagonal_direction(cursor_manager, mock_controller, mock_widget):
    mock_controller.is_selecting = True
    mock_controller.active_handle = 'expand_br'
    mock_controller.click_start_pos = QPoint(50, 50)
    cursor_manager.update_cursor(QPoint(100, 100))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeFDiagCursor)


def test_expanding_tl_diagonal_direction(cursor_manager, mock_controller, mock_widget):
    mock_controller.is_selecting = True
    mock_controller.active_handle = 'expand_tl'
    mock_controller.click_start_pos = QPoint(50, 50)
    cursor_manager.update_cursor(QPoint(0, 0))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeFDiagCursor)


def test_expanding_tr_diagonal_direction(cursor_manager, mock_controller, mock_widget):
    mock_controller.is_selecting = True
    mock_controller.active_handle = 'expand_tr'
    mock_controller.click_start_pos = QPoint(50, 50)
    cursor_manager.update_cursor(QPoint(100, 0))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeBDiagCursor)


def test_expanding_bl_diagonal_direction(cursor_manager, mock_controller, mock_widget):
    mock_controller.is_selecting = True
    mock_controller.active_handle = 'expand_bl'
    mock_controller.click_start_pos = QPoint(50, 50)
    cursor_manager.update_cursor(QPoint(0, 100))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeBDiagCursor)


def test_expanding_top_edge_always_vertical(cursor_manager, mock_controller, mock_widget):
    mock_controller.is_selecting = True
    mock_controller.active_handle = 'expand_t'
    mock_controller.click_start_pos = QPoint(50, 50)
    cursor_manager.update_cursor(QPoint(100, 100))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeVerCursor)


def test_expanding_bottom_edge_always_vertical(cursor_manager, mock_controller, mock_widget):
    mock_controller.is_selecting = True
    mock_controller.active_handle = 'expand_b'
    mock_controller.click_start_pos = QPoint(50, 50)
    cursor_manager.update_cursor(QPoint(100, 100))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeVerCursor)


def test_expanding_left_edge_always_horizontal(cursor_manager, mock_controller, mock_widget):
    mock_controller.is_selecting = True
    mock_controller.active_handle = 'expand_l'
    mock_controller.click_start_pos = QPoint(50, 50)
    cursor_manager.update_cursor(QPoint(0, 0))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeHorCursor)


def test_expanding_right_edge_always_horizontal(cursor_manager, mock_controller, mock_widget):
    mock_controller.is_selecting = True
    mock_controller.active_handle = 'expand_r'
    mock_controller.click_start_pos = QPoint(50, 50)
    cursor_manager.update_cursor(QPoint(100, 100))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeHorCursor)


def test_expanding_no_click_start_cross(cursor_manager, mock_controller, mock_widget):
    mock_controller.is_selecting = True
    mock_controller.active_handle = 'expand_br'
    mock_controller.click_start_pos = QPoint(0, 0)
    cursor_manager.update_cursor(QPoint(100, 100))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.CrossCursor)


def test_annotation_selected_pointer_tool_on_rotate_handle(cursor_manager, mock_controller, mock_widget):
    annotation_manager = MagicMock()
    annotation_manager.current_tool = 'pointer'
    mock_controller.annotation_manager = annotation_manager
    selected_item = MagicMock()
    selected_item.selected = True
    selected_item.get_handle_at.return_value = 'rotate'
    annotation_manager.selected_item = selected_item
    cursor_manager.update_cursor(QPoint(50, 50))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeAllCursor)


def test_annotation_selected_pointer_tool_on_tl_handle(cursor_manager, mock_controller, mock_widget):
    annotation_manager = MagicMock()
    annotation_manager.current_tool = 'pointer'
    mock_controller.annotation_manager = annotation_manager
    selected_item = MagicMock()
    selected_item.selected = True
    selected_item.get_handle_at.return_value = 'tl'
    annotation_manager.selected_item = selected_item
    cursor_manager.update_cursor(QPoint(50, 50))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeFDiagCursor)


def test_annotation_selected_pointer_tool_on_br_handle(cursor_manager, mock_controller, mock_widget):
    annotation_manager = MagicMock()
    annotation_manager.current_tool = 'pointer'
    mock_controller.annotation_manager = annotation_manager
    selected_item = MagicMock()
    selected_item.selected = True
    selected_item.get_handle_at.return_value = 'br'
    annotation_manager.selected_item = selected_item
    cursor_manager.update_cursor(QPoint(50, 50))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeFDiagCursor)


def test_annotation_selected_pointer_tool_on_tr_handle(cursor_manager, mock_controller, mock_widget):
    annotation_manager = MagicMock()
    annotation_manager.current_tool = 'pointer'
    mock_controller.annotation_manager = annotation_manager
    selected_item = MagicMock()
    selected_item.selected = True
    selected_item.get_handle_at.return_value = 'tr'
    annotation_manager.selected_item = selected_item
    cursor_manager.update_cursor(QPoint(50, 50))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeBDiagCursor)


def test_annotation_selected_pointer_tool_on_bl_handle(cursor_manager, mock_controller, mock_widget):
    annotation_manager = MagicMock()
    annotation_manager.current_tool = 'pointer'
    mock_controller.annotation_manager = annotation_manager
    selected_item = MagicMock()
    selected_item.selected = True
    selected_item.get_handle_at.return_value = 'bl'
    annotation_manager.selected_item = selected_item
    cursor_manager.update_cursor(QPoint(50, 50))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.SizeBDiagCursor)


def test_annotation_selected_not_pointer_tool(cursor_manager, mock_controller, mock_widget):
    annotation_manager = MagicMock()
    annotation_manager.current_tool = 'pencil'
    mock_controller.annotation_manager = annotation_manager
    selected_item = MagicMock()
    selected_item.selected = True
    selected_item.get_handle_at.return_value = None
    annotation_manager.selected_item = selected_item
    cursor_manager.update_cursor(QPoint(50, 50))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.CrossCursor)


def test_annotation_selected_no_item(cursor_manager, mock_controller, mock_widget):
    annotation_manager = MagicMock()
    annotation_manager.current_tool = 'pointer'
    mock_controller.annotation_manager = annotation_manager
    annotation_manager.selected_item = None
    mock_controller.selection_rect = QRect()
    cursor_manager.update_cursor(QPoint(50, 50))
    mock_widget.setCursor.assert_called_with(Qt.CursorShape.CrossCursor)
