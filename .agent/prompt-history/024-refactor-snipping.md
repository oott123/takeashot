重构一下 snipping widget 的结构。

1. 窗口单独弄。搞一个 snipping window 的文件出来。然后把 snipping widget 当成子控件放进去。paint 就放在 widget 里，
不要 paint 一整个窗口。注意原来挂在 snipping widget 上面的窗口属性不要弄错了。
2. 把 toolbar 做成一个 snipping window 的子 widget。然后把里面的 paint 逻辑，都改成用原生控件实现。

---

现在的问题是，工具条只在主窗口出现。当它应该在其它窗口的时候，就看不到了。

---

好像把复制功能搞坏了，Traceback (most recent call last):
  File "/home/alice/Repos/takeashot/snipping_widget.py", line 211, in keyPressEvent
    self.controller.capture_selection()
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~^^
  File "/home/alice/Repos/takeashot/main.py", line 218, in capture_selection
    dpr = snipper.full_pixmap.devicePixelRatio()
          ^^^^^^^^^^^^^^^^^^^
AttributeError: 'SnippingWindow' object has no attribute 'full_pixmap'
