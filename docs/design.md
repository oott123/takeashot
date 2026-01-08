# 设计文档

## 项目用途

这是一个专为 KDE Wayland 环境设计的截图工具。它使用 PyQt5 构建，支持多屏幕截图、区域选择以及智能窗口吸附功能。工具通过 D-Bus 与 KWin 交互获取屏幕截图和窗口信息。

## 选区状态

### 1. 拟选择状态 (Pending Selection)
- **定义**: 用户鼠标悬停在某个窗口上，但尚未点击确认时的预览状态。
- **状态变量**: `pending_selection_rect` 有值 (非空)，`selection_rect` 为空。
- **表现**: 显示对应窗口的蓝色边框，但**不显示**调整手柄 (Resize Handles) 和工具栏。
- **操作**: 点击鼠标左键将确认该区域为正式选区。

### 2. 有选区状态 (Has Selection)
- **定义**: 用户已确认了一个具体的截图区域（通过拖拽创建或点击“拟选择”区域确认）。
- **状态变量**: `selection_rect` 有值 (非空)。
- **表现**: 显示选区的蓝色边框，显示 8 个调整手柄，并在选区附近显示工具栏 (Toolbar)。选区以外区域显示半透明遮罩。
- **操作**: 可以拖拽手柄调整大小，拖拽选区移动位置，按 Enter 或点击工具栏按钮完成截图。

### 3. 无选区状态 (No Selection)
- **定义**: 初始状态或取消选区后的状态，没有任何区域被选中或预览。
- **状态变量**: `selection_rect` 和 `pending_selection_rect` 均为空。
- **表现**: 全屏显示半透明遮罩，鼠标光标为十字准星。
- **操作**: 鼠标拖拽可创建新选区，移动鼠标可寻找窗口触发“拟选择状态”。

## 支持环境

- **OS**: Linux
- **Desktop Environment**: KDE Plasma (Wayland Session)
- **Dependencies**: 依赖 KWin 的 D-Bus 接口 (`org.kde.KWin.ScreenShot2`, 脚本接口) 获取截图和窗口列表。

## 相关代码位置

### `main.py`
- **`ScreenshotApp` 类**: 核心控制器，管理全局状态和逻辑。
    - **状态管理**: 维护 `selection_rect` (实选区), `pending_selection_rect` (拟选区), `windows` (窗口列表)。
    - **逻辑处理**: `on_mouse_press`, `on_mouse_move`, `capture_selection` (截图合成与剪贴板操作), `_start_window_snapping` (启动窗口列表获取)。
    - **交互**: 处理从 SnippingWidget 转发过来的鼠标事件，决定状态流转。

### `snipping_widget.py`
- **`SnippingWindow` 类**: 每个屏幕一个的顶层全屏窗口。
    - **职责**: 作为容器，包含 `SnippingWidget` 和 `Toolbar`。管理工具栏的显示和定位 (`update_toolbar_position`)，处理窗口级别的按键 (Esc, Enter)。
- **`SnippingWidget` 类**: 填充窗口的主要绘图组件。
    - **职责**: 负责绘制 (`paintEvent`)，包括背景图、半透明遮罩、选区边框、拟选区边框和手柄。
    - **事件**: 捕获鼠标事件 (`mousePressEvent` 等) 并**转发**给 `ScreenshotApp` 控制器处理。

### `Toolbar.qml`
- **`Toolbar` 组件**: 使用 QML 实现的悬浮工具栏。
    - **职责**: 负责 UI 渲染 (使用 Canvas 绘制图标) 和交互事件。包含透明顶部内边距 (`topPadding`) 以支持 Tooltip 显示。
    - **交互**: 解耦事件处理，不直接暴露按钮，而是发送信号 (`cancelRequested`, `saveRequested`, `confirmRequested`) 供外部连接。

### `window_lister.py`
- **`WindowLister` 类**: 负责通过 KWin 脚本接口获取当前打开的窗口列表（位置和大小）。
    - **职责**: 异步执行 KWin 脚本，通过 D-Bus 信号接收窗口数据，用于吸附功能。

### `screenshot_backend.py`
- **`ScreenshotBackend` 类**: 负责底层的屏幕抓取。
    - **职责**: 封装 `org.kde.KWin.ScreenShot2` 接口调用，提供 `capture_screen` (单屏) 和 `capture_workspace` (全屏) 方法。