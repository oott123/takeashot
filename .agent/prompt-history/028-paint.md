增加标注功能
1. 工具栏上新增：指针工具、铅笔工具、直线工具、矩形工具、椭圆工具
2. 选择绘图工具后可以在选区内绘图，选择指针工具可以选择绘制轨迹
3. 绘制轨迹可以移动、缩放、旋转、删除
4. 点击确认后，绘制轨迹应和选区图片一起出现在剪贴板中
5. 代码应可扩展，后续增加工具应该简单方便

---

Tool selected: line
Traceback (most recent call last):
  File "/home/alice/Repos/takeashot/snipping_widget.py", line 132, in mousePressEvent
    self.controller.on_mouse_press(event.globalPos())
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~^^^^^^^^^^^^^^^^^^^
  File "/home/alice/Repos/takeashot/main.py", line 302, in on_mouse_press
    if self.annotation_manager.handle_mouse_press(global_pos):
       ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~^^^^^^^^^^^^
  File "/home/alice/Repos/takeashot/annotations/manager.py", line 92, in handle_mouse_press
    self.active_item = LineItem(pos, self.current_color, self.current_width)
                       ~~~~~~~~^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "/home/alice/Repos/takeashot/annotations/items.py", line 152, in __init__
    super().__init__(start_pos, color, width)
    ~~~~~~~~~~~~~~~~^^^^^^^^^^^^^^^^^^^^^^^^^
  File "/home/alice/Repos/takeashot/annotations/items.py", line 12, in __init__
    self.rect = QRectF(start_pos, QSize(0, 0)) # Bounding rect in local coords (relative to pos if we were using it that way, but here simpler to just store geom)
                                  ^^^^^
NameError: name 'QSize' is not defined

---

Tool selected: line
Traceback (most recent call last):
  File "/home/alice/Repos/takeashot/snipping_widget.py", line 132, in mousePressEvent
    self.controller.on_mouse_press(event.globalPos())
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~^^^^^^^^^^^^^^^^^^^
  File "/home/alice/Repos/takeashot/main.py", line 302, in on_mouse_press
    if self.annotation_manager.handle_mouse_press(global_pos):
       ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~^^^^^^^^^^^^
  File "/home/alice/Repos/takeashot/annotations/manager.py", line 92, in handle_mouse_press
    self.active_item = LineItem(pos, self.current_color, self.current_width)
                       ~~~~~~~~^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "/home/alice/Repos/takeashot/annotations/items.py", line 152, in __init__
    super().__init__(start_pos, color, width)
    ~~~~~~~~~~~~~~~~^^^^^^^^^^^^^^^^^^^^^^^^^
  File "/home/alice/Repos/takeashot/annotations/items.py", line 12, in __init__
    self.rect = QRectF(start_pos, QSize(0, 0)) # Bounding rect in local coords (relative to pos if we were using it that way, but here simpler to just store geom)
                ~~~~~~^^^^^^^^^^^^^^^^^^^^^^^^
TypeError: arguments did not match any overloaded call:
  QRectF(): too many arguments
  QRectF(atopLeft: Union[QPointF, QPoint], asize: QSizeF): argument 2 has unexpected type 'QSize'
  QRectF(atopLeft: Union[QPointF, QPoint], abottomRight: Union[QPointF, QPoint]): argument 2 has unexpected type 'QSize'
  QRectF(aleft: float, atop: float, awidth: float, aheight: float): argument 1 has unexpected type 'QPoint'
  QRectF(r: QRect): argument 1 has unexpected type 'QPoint'
  QRectF(a0: QRectF): argument 1 has unexpected type 'QPoint'
zsh: abort (core dumped)  python main.py