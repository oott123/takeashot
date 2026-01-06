# 设计文档

## 项目用途

基于 PyQt5 的截图工具，支持窗口吸附功能，用于在 KDE Wayland 环境下进行屏幕区域选择和截图。

## 选区状态

### 拟选择状态（Pending Selection）

鼠标悬停在窗口上时，显示窗口边框预览，但还未确认选中。此时 `pending_selection_rect` 有值，`selection_rect` 为空，显示蓝色边框但不显示调整手柄。

### 有选区状态（Has Selection）

用户已经确认了选区（可以通过拖拽创建或点击窗口确认），此时 `selection_rect` 有值，可以调整大小或移动，按 Enter 确认截图，显示蓝色边框和调整手柄。

### 无选区状态（No Selection）

既没有拟选择也没有实际选区，此时 `selection_rect` 和 `pending_selection_rect` 都为空，显示半透明遮罩。

## 支持环境

KDE Wayland only（依赖 KWin 的 D-Bus 接口）

## 相关代码位置

### main.py - ScreenshotApp 类

- `selection_rect`：实际选区
- `pending_selection_rect`：拟选区
- `pending_window`：鼠标下的窗口
- `on_mouse_press`：鼠标按下事件处理
- `on_mouse_move`：鼠标移动事件处理
- `on_mouse_release`：鼠标释放事件处理
- `capture_selection`：截图功能
- `cancel_selection`：取消选区
- `should_exit`：判断是否退出
- `get_pending_selection_rect`：获取拟选区
- `_get_window_at`：获取鼠标位置的窗口
- `_on_windows_ready`：窗口列表获取完成回调
- `get_handle_rects`：获取调整手柄位置
- `get_handle_at`：获取鼠标位置的调整手柄

### snipping_widget.py - SnippingWidget 类

- `paintEvent`：绘制截图界面（包括背景、选区、拟选区、调整手柄）
- `mousePressEvent`：鼠标按下事件
- `mouseMoveEvent`：鼠标移动事件
- `mouseReleaseEvent`：鼠标释放事件
- `keyPressEvent`：键盘事件处理
- `draw_handles`：绘制调整手柄

### window_lister.py

- `WindowLister` 类：窗口列表获取器
  - `get_windows_async`：异步获取窗口列表
  - `_on_windows_received`：窗口数据接收完成回调
- `WindowListReceiver` 类：DBus 接收器
  - `receive`：接收 KWin 脚本返回的窗口数据

### screenshot_backend.py - ScreenshotBackend 类

- `capture_workspace`：截取整个工作区
- `capture_screen`：截取单个屏幕