这是一个截图工具，现在这个截图工具用一个大窗口覆盖所有显示器，这是不对的，应该有多少显示器创建多少个窗口，然后只有其中一个窗口可以选区复制，然后窗口需要有 Qt.WindowFullScreen 标记。

---

现在截图显示区域不正确，只能画出来左上角一小块，一旦选择，Traceback (most recent call last):
  File "/home/alice/Repos/takeashot/main.py", line 93, in on_selection_started
    sender = self.sender()
             ^^^^^^^^^^^
AttributeError: 'ScreenshotApp' object has no attribute 'sender'
Aborted                    (core dumped) python3 main.py
。另外两个屏幕上显示的内容也不正确。可能是没有考虑 hidpi ？

---

显示、选区都是对的。现在的问题是，选区之后确认，复制到剪贴板里的图像不对，只有左上角大概1/4，应该还是 hidpi 的问题。