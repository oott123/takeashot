重构鼠标光标形状相关代码。
1. 将所有原来配置鼠标光标形状的代码全部移除。
2. 将鼠标光标形状的代码放到一起，然后按如下规则执行：
    1. 当未选区、或是选区待确认时，鼠标光标为十字形。
    2. 当已选区时，检查鼠标是否在选区内（和选区边缘重叠不算在选区内）
        a. 如果在选区内，则根据工具选择不同光标：
            i. 选择/移动工具，光标为移动光标
            ii. 标注绘制工具，光标为十字形
        b. 如果不在选区内（包括在选区拖动句柄、外部等情况），显示为八方向尺寸调整光标
    3. 当鼠标在工具栏时，让工具栏 QML 控制光标形状即可
3. 这部分代码需要写在一个专门的模块里，不要再到处判断了

---

1. 手柄只是一个 UI，处理上不需要特殊处理，因为不管在手柄上拖动，还是在边框上拖动，还是点选区外部，
都是一样的缩放逻辑，所以光标上也应该都一样处理
2. 通过 controller.annotation_manager.current_tool 获取就挺好，需要的话可以封装

---

八方向中，鼠标按下调整尺寸的状态下，上下左右四个方向处理得不对，鼠标拖动上边的时候，应该一直显示上下箭头，而不管鼠标
当前在哪里，因为这个时候无论如何都只缩放一条边，所以显示为上下箭头就行了。下、左、右三边同理。

---

接下来处理编辑标注的时候的问题。当工具是选择/移动工具，并且有标注选中、且鼠标放到对应标注的手柄上的时候，
光标应该改为对应的八方向和旋转鼠标。

---

Traceback (most recent call last):
  File "/home/alice/Repos/takeashot/snipping_widget.py", line 146, in mouseMoveEvent
    self.cursor_manager.update_cursor(global_pos)
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~^^^^^^^^^^^^
  File "/home/alice/Repos/takeashot/cursor_manager.py", line 23, in update_cursor
    handle = selected_item.get_handle_at(global_pos)
  File "/home/alice/Repos/takeashot/annotations/items.py", line 84, in get_handle_at
    if (local_pos - rot_handle).manhattanLength() < h:
        ~~~~~~~~~~^~~~~~~~~~~~
TypeError: unsupported operand type(s) for -: 'QPoint' and 'QPointF'