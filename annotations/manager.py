import math
from PyQt5.QtCore import Qt, QPoint, QPointF, QRectF
from PyQt5.QtGui import QColor
from .items import RectItem, EllipseItem, LineItem, StrokeItem

class AnnotationManager:
    def __init__(self):
        self.items = []
        self.current_tool = 'pointer' # pointer, pencil, line, rect, ellipse
        self.current_color = Qt.red
        self.current_width = 3
        
        self.active_item = None # Item being drawn or manipulated
        self.selected_item = None # Selected item for editing
        
        # Dragging state
        self.drag_start_pos = QPointF()
        self.is_drawing = False
        self.is_moving = False
        self.is_resizing = False
        self.is_rotating = False
        self.active_handle = None
        
    def set_tool(self, tool_name):
        self.current_tool = tool_name
        self.selected_item = None # Deselect when changing tools (optional)
        self.update_snippets() # Trigger repaint to clear selection UI

    # Optional hook to update UI
    def update_snippets(self):
        pass 

    def delete_selected_item(self):
        if self.selected_item and self.selected_item in self.items:
            self.items.remove(self.selected_item)
            self.selected_item = None
            return True
        return False

    def handle_mouse_press(self, pos):
        """
        Returns True if the event was handled by the annotation system.
        Returns False if the caller should handle it (e.g. window selection).
        """
        self.drag_start_pos = pos
        
        if self.current_tool == 'pointer':
            # 1. Check handles of selected item
            if self.selected_item:
                handle = self.selected_item.get_handle_at(pos)
                if handle:
                    self.active_item = self.selected_item
                    self.active_handle = handle
                    if handle == 'rotate':
                        self.is_rotating = True
                    else:
                        self.is_resizing = True
                    return True
            
            # 2. Check hit on items (top to bottom)
            clicked_item = None
            for item in reversed(self.items):
                if item.contains(pos):
                    clicked_item = item
                    break
            
            if clicked_item:
                self.selected_item = clicked_item
                self.selected_item.selected = True
                self.is_moving = True
                self.active_item = clicked_item
                # Deselect others
                for item in self.items:
                    if item != clicked_item:
                        item.selected = False
                return True
            else:
                # Clicked empty space
                self.selected_item = None
                for item in self.items:
                    item.selected = False
                return False 
                
        else:
            # Drawing a new shape
            self.is_drawing = True
            if self.current_tool == 'rect':
                self.active_item = RectItem(pos, self.current_color, self.current_width)
            elif self.current_tool == 'ellipse':
                self.active_item = EllipseItem(pos, self.current_color, self.current_width)
            elif self.current_tool == 'line':
                self.active_item = LineItem(pos, self.current_color, self.current_width)
            elif self.current_tool == 'pencil':
                self.active_item = StrokeItem(pos, self.current_color, self.current_width)
                
            if self.active_item:
                self.items.append(self.active_item)
                return True
                
        return False

    def handle_mouse_move(self, pos):
        if self.is_drawing and self.active_item:
            if isinstance(self.active_item, StrokeItem):
                self.active_item.add_point(pos)
            else:
                self.active_item.update_geometry(self.drag_start_pos, pos)
            return True
            
        if self.active_item:
            if self.is_moving:
                delta = pos - self.drag_start_pos
                self.active_item.move(delta)
                self.drag_start_pos = pos
                return True
            elif self.is_resizing:
                self.active_item.resize(self.active_handle, pos)
                return True
            elif self.is_rotating:
                center = self.active_item.rect.center()
                # Local vector from center to previous pos
                v1 = self.drag_start_pos - center
                # Local vector from center to current pos
                v2 = pos - center
                
                a1 = math.atan2(v1.y(), v1.x())
                a2 = math.atan2(v2.y(), v2.x())
                
                delta_angle = math.degrees(a2 - a1)
                self.active_item.rotate(delta_angle)
                
                # Update drag start for next increment
                # Actually, iterating delta is fine if we update drag_start_pos?
                # Yes, because next move compares to this pos.
                self.drag_start_pos = pos 
                return True
            
        return False

    def handle_mouse_release(self, pos):
        handled = self.is_drawing or self.is_moving or self.is_resizing or self.is_rotating
        
        self.is_drawing = False
        self.is_moving = False
        self.is_resizing = False
        self.is_rotating = False
        self.active_item = None # Clear active item (keep selected item)
        self.active_handle = None
        
        return handled

    def draw_annotations(self, painter):
        for item in self.items:
            item.draw(painter)

    def reset(self):
        self.items = []
        self.current_tool = 'pointer'
        self.active_item = None
        self.selected_item = None
        self.is_drawing = False
        self.is_moving = False
        self.is_resizing = False
        self.is_rotating = False
        self.active_handle = None
