现状：在鼠标悬浮在某个窗口上的时候（pending 选区），如果按 Esc，这个 pending 会被清空，但不会退出。
需求：修改为直接退出。

同时，将鼠标右键和 Esc 的处理函数放到一起，使它们无论什么时候都执行同一个操作。

---

Traceback (most recent call last):
  File "/home/alice/Repos/takeashot/snipping_widget.py", line 128, in keyPressEvent
    self.handle_cancel_or_exit()
    ^^^^^^^^^^^^^^^^^^^^^^^^^^
AttributeError: 'SnippingWidget' object has no attribute 'handle_cancel_or_exit'