# Proposal — turn off KWin's open/close animation on our overlay

当 overlay 显示时，KWin 会对它应用窗口打开/关闭动画。效果是：带着屏幕截图的 overlay 窗口在屏幕上以 scale-in 的方式出现，看起来非常难看（因为 overlay 的内容是"冻结的桌面"，缩放效果会露馅）。

希望：让 overlay 窗口不经过 KWin 的打开动画。关闭动画如果也能一并关掉更好。
