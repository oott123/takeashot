目前选区只能在单一屏幕里存在。我们需要将选区功能设计为可以跨屏渲染：选区坐标在内部记录为相对虚拟桌面的坐标，如果坐标跨屏幕，则两个屏幕上都各自显示各自的部分。需要注意的是，缩放句柄不要在屏幕边缘重复，仔细想想这个数据结构如何设计、送到剪贴板的时候如何拼接，这会很难做，需要输出更加详细的方案。

---

Traceback (most recent call last):
  File "/home/alice/Repos/takeashot/snipping_widget.py", line 53, in paintEvent
    region_all = QRegion(self.rect())
                 ^^^^^^^
NameError: name 'QRegion' is not defined

---

Traceback (most recent call last):
  File "/home/alice/Repos/takeashot/snipping_widget.py", line 57, in paintEvent
    for rect in region_overlay:
                ^^^^^^^^^^^^^^
TypeError: 'QRegion' object is not iterable
