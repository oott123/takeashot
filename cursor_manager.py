from PyQt6.QtCore import Qt, QRect, QPointF
from PyQt6.QtWidgets import QWidget


class CursorManager:
    def __init__(self, controller, widget: QWidget):
        self.controller = controller
        self.widget = widget

    def update_cursor(self, global_pos):
        selection_rect = self.controller.selection_rect
        active_handle = self.controller.active_handle if self.controller.is_selecting else None
        is_expanding = active_handle and active_handle.startswith('expand_')

        if is_expanding:
            self._update_cursor_during_expand(global_pos)
            return

        annotation_manager = getattr(self.controller, 'annotation_manager', None)
        
        # Check for edit tool - show annotation cursors when hovering over annotations
        if annotation_manager and annotation_manager.current_tool == 'edit':
            selected_item = getattr(annotation_manager, 'selected_item', None)
            if selected_item and selected_item.selected:
                handle = selected_item.get_handle_at(QPointF(global_pos))
                if handle:
                    self._set_annotation_cursor(handle)
                    return
            
            # Check if mouse is over any annotation (for move cursor)
            for item in reversed(annotation_manager.items):
                if item.contains(QPointF(global_pos)):
                    self.widget.setCursor(Qt.CursorShape.SizeAllCursor)
                    return
            
            # No annotation under mouse - show normal arrow cursor
            self.widget.setCursor(Qt.CursorShape.ArrowCursor)
            return
        
        # For pointer tool and drawing tools, use selection-based cursors
        if not selection_rect.isNull():
            if selection_rect.contains(global_pos, proper=True):
                if self._is_pointer_tool():
                    self.widget.setCursor(Qt.CursorShape.SizeAllCursor)
                else:
                    self.widget.setCursor(Qt.CursorShape.CrossCursor)
            else:
                self._update_cursor_for_outside(global_pos)
        else:
            self.widget.setCursor(Qt.CursorShape.CrossCursor)

    def _set_annotation_cursor(self, handle):
        if handle == 'rotate':
            self.widget.setCursor(Qt.CursorShape.SizeAllCursor)
        elif handle in ['tl', 'br']:
            self.widget.setCursor(Qt.CursorShape.SizeFDiagCursor)
        elif handle in ['tr', 'bl']:
            self.widget.setCursor(Qt.CursorShape.SizeBDiagCursor)

    def _is_pointer_tool(self):
        tool = getattr(self.controller.annotation_manager, 'current_tool', None)
        return tool == 'pointer'

    def _update_cursor_during_expand(self, global_pos):
        active_handle = self.controller.active_handle

        if active_handle == 'expand_t':
            self.widget.setCursor(Qt.CursorShape.SizeVerCursor)
            return
        elif active_handle == 'expand_b':
            self.widget.setCursor(Qt.CursorShape.SizeVerCursor)
            return
        elif active_handle == 'expand_l':
            self.widget.setCursor(Qt.CursorShape.SizeHorCursor)
            return
        elif active_handle == 'expand_r':
            self.widget.setCursor(Qt.CursorShape.SizeHorCursor)
            return

        click_start_pos = self.controller.click_start_pos
        if click_start_pos.isNull():
            self.widget.setCursor(Qt.CursorShape.CrossCursor)
            return

        dx = global_pos.x() - click_start_pos.x()
        dy = global_pos.y() - click_start_pos.y()

        is_left = dx < 0
        is_right = dx > 0
        is_top = dy < 0
        is_bottom = dy > 0

        if is_left and is_top:
            self.widget.setCursor(Qt.CursorShape.SizeFDiagCursor)
        elif is_right and is_top:
            self.widget.setCursor(Qt.CursorShape.SizeBDiagCursor)
        elif is_left and is_bottom:
            self.widget.setCursor(Qt.CursorShape.SizeBDiagCursor)
        elif is_right and is_bottom:
            self.widget.setCursor(Qt.CursorShape.SizeFDiagCursor)
        else:
            self.widget.setCursor(Qt.CursorShape.CrossCursor)

    def _update_cursor_for_outside(self, global_pos):
        selection_rect = self.controller.selection_rect

        if selection_rect.isNull():
            self.widget.setCursor(Qt.CursorShape.CrossCursor)
            return

        x = global_pos.x()
        y = global_pos.y()
        left = selection_rect.left()
        right = selection_rect.right()
        top = selection_rect.top()
        bottom = selection_rect.bottom()

        is_left = x < left
        is_right = x > right
        is_top = y < top
        is_bottom = y > bottom

        if is_left and is_top:
            self.widget.setCursor(Qt.CursorShape.SizeFDiagCursor)
        elif is_right and is_top:
            self.widget.setCursor(Qt.CursorShape.SizeBDiagCursor)
        elif is_left and is_bottom:
            self.widget.setCursor(Qt.CursorShape.SizeBDiagCursor)
        elif is_right and is_bottom:
            self.widget.setCursor(Qt.CursorShape.SizeFDiagCursor)
        elif is_left or is_right:
            self.widget.setCursor(Qt.CursorShape.SizeHorCursor)
        elif is_top or is_bottom:
            self.widget.setCursor(Qt.CursorShape.SizeVerCursor)
        else:
            self.widget.setCursor(Qt.CursorShape.CrossCursor)
