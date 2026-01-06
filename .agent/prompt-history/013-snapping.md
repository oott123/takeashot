这是一个截图工具。现在，我希望它在还没有选区的时候，鼠标可以吸附（snapping）窗口。

这需要：
1. 在获取截图的几乎同时，获取窗口列表。获取窗口列表的原型脚本已经验证过，放在了 `prototypes/list_windows.py` 里，
可以参考实现（复制代码到你的实现里，但不要直接调用）。原理是注册一个 dbus receiver，然后调用 KWin Script 获取列表，
发到 dbus 上。这个原型脚本输出的坐标和尺寸都是逻辑尺寸，不是物理尺寸。
2. 在没有选区的时候，鼠标如果在某个窗口的坐标范围内，则自动将选区设置为这个窗口的尺寸
3. 此时称之为“拟选中”状态。如果用户单击鼠标，则确认选中。如果用户开始拖动，则就像没有拟选中一样，直接跟随用户的拖拽创建新的选区
4. 原本按 Esc 键或者右键就会直接退出截图。现在，如果用户在已经有选区的时候按 Esc 或者右键，则取消选区。
没有选区、或者拟选区的时候，则直接退出。

注意：
1. 实现的时候注意考虑 hidpi 的情况。
2. 获取窗口列表需要注册 dbus 接收器，这可以在最开始启动的时候就完成，并且一直保持。注意接收器的名字保持有意义。
3. 获取窗口列表本身需要一定时间，所以在启动截图的时候，直接进入截图状态，然后异步发 dbus 获取，再异步等到
获取完成之后，再启用吸附功能。
4. 用 Qt 的主循环、事件来处理异步，不要像原型脚本里面那样自己启动一个主循环了，也不要自己开线程，用好 Qt 的机制。

---

Failed to start window list retrieval: module 'dbus' has no attribute 'mainloop'

---

Failed to start window list retrieval: Invalid bus name 'com.takeashot.screenshot.206224': a digit may not follow '.' except in a unique name starting with ':'

---

Failed to start window list retrieval: To make asynchronous calls, receive signals or export objects, D-Bus connections must be attached to a main loop by passing mainloop=... to the constructor or calling dbus.set_default_main_loop(...)

---

Failed to start window list retrieval: To make asynchronous calls, receive signals or export objects, D-Bus connections must be attached to a main loop by passing mainloop=... to the constructor or calling dbus.set_default_main_loop(...) 还是报这个错

---

Window snapping enabled with 9 windows
Traceback (most recent call last):
  File "/home/alice/Repos/takeashot/snipping_widget.py", line 104, in mouseMoveEvent
    self.controller.on_mouse_move(global_pos)
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~^^^^^^^^^^^^
  File "/home/alice/Repos/takeashot/main.py", line 294, in on_mouse_move
    snapped_window = self._get_window_at(global_pos)
  File "/home/alice/Repos/takeashot/main.py", line 190, in _get_window_at
    window_rect = QRect(x, y, w, h)
TypeError: arguments did not match any overloaded call:
  QRect(): too many arguments
  QRect(aleft: int, atop: int, awidth: int, aheight: int): argument 1 has unexpected type 'float'
  QRect(atopLeft: QPoint, abottomRight: QPoint): argument 1 has unexpected type 'float'
  QRect(atopLeft: QPoint, asize: QSize): argument 1 has unexpected type 'float'
  QRect(a0: QRect): argument 1 has unexpected type 'float'
zsh: abort (core dumped)  python main.py