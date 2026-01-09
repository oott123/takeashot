修复以下问题：
在截图的时候，选择一个标注工具比如直线，然后取消选区。现状：无法进行下一次选区选区；预期：能和第一次进入一样正常选区。

---

我只想取消选择工具，不想把整个标注都 reset 掉

---

工具是取消掉了，但是工具栏的状态还不对，这个也要重置一下

---

QMetaObject::invokeMethod: No such method Toolbar_QMLTYPE_3::selectTool(QString)
Candidates are:
    selectTool(QVariant)
Traceback (most recent call last):
  File "/home/alice/Repos/takeashot/snipping_widget.py", line 136, in mousePressEvent
    self.window().handle_cancel_or_exit()
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~^^
  File "/home/alice/Repos/takeashot/snipping_widget.py", line 240, in handle_cancel_or_exit
    elif not self.controller.cancel_selection():
             ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~^^
  File "/home/alice/Repos/takeashot/main.py", line 608, in cancel_selection
    snipper.reset_toolbar_tool()
    ~~~~~~~~~~~~~~~~~~~~~~~~~~^^
  File "/home/alice/Repos/takeashot/snipping_widget.py", line 265, in reset_toolbar_tool
    QMetaObject.invokeMethod(root, "selectTool", Qt.DirectConnection, Q_ARG(str, tool_name))
    ~~~~~~~~~~~~~~~~~~~~~~~~^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
RuntimeError: QMetaObject.invokeMethod() call failed
zsh: abort (core dumped)  python main.py