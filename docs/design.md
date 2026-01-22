# 设计文档

此文档旨在帮助 AI agent 快速理解本项目架构与逻辑。

## 1. 项目用途
这是一个专为 KDE Wayland 环境设计的截图工具。它的主要功能包括：
- 多屏幕截图支持
- 区域选择与调整
- 窗口吸附（智能识别并选中窗口）
- 简单的标注功能（如画笔、箭头等）
- 剪贴板集成

## 2. 选区状态定义

在 `ScreenshotApp` (main.py) 中，系统维护着几种互斥的选区状态：

- **无选区 (No Selection)**
  - `selection_rect` 为空且 `pending_selection_rect` 为空。
  - 用户未进行任何操作，界面显示全屏遮罩。
  - 亦即初始状态。

- **拟选择 (Pending Selection)**
  - `pending_selection_rect` 有值，但 `selection_rect` 为空。
  - **触发条件**：启用了窗口吸附 (`snapping_enabled`=True)，且鼠标悬停在某个窗口区域上。
  - **表现**：显示蓝色边框预览，但**不显示**调整手柄（Resize Handles）。
  - **行为**：单击即可确认该区域为正式选区。

- **有选区 (Has Selection)**
  - `selection_rect` 有值。
  - **触发条件**：用户通过拖拽创建了选区，或确认了拟选择区域。
  - **表现**：显示选区高亮、遮罩挖空，并显示调整手柄。
  - **行为**：可以调整大小、移动位置、进行标注或确认截图。

## 3. 支持环境
- **操作系统**: Linux
- **桌面环境**: KDE Plasma (Wayland Session)
- **依赖说明**: 强依赖 KWin 的 DBus 接口与其 Scripting API 获取窗口信息，因此**仅支持 KDE Wayland**。

## 4. 关键代码位置

| 功能模块 | 类名/函数名 | 文件位置 |
| :--- | :--- | :--- |
| **程序入口与核心控制** | `ScreenshotApp` | `main.py` |
| **截图窗口 UI** | `SnippingWindow`, `SnippingWidget` | `snipping_widget.py` |
| **标注管理** | `AnnotationManager` | `annotations/manager.py` |
| **窗口列表获取 (KWin)** | `WindowLister` | `window_lister.py` |
| **光标状态管理** | `CursorManager` | `cursor_manager.py` |
| **DBus 管理 (单例/通信)** | `DbusManager` | `dbus_manager.py` |
| **底层截图实现** | `ScreenshotBackend` | `screenshot_backend.py` |
| **全局输入监听** | `GlobalInputMonitor` | `input_monitor.py` |

## 5. 鼠标光标管理
光标状态由 `CursorManager` 统一管理。

- **调用**：`SnippingWidget` 在 `mouseMoveEvent` 中调用 `cursor_manager.update_cursor(global_pos)`。
- **逻辑**：
  - 判断鼠标是否在选区的调整手柄上 -> 显示对应的调整光标 (如 `SizeFDiagCursor`)。
  - 判断鼠标是否在选区内 -> 显示移动光标 (`SizeAllCursor`)。
  - 判断是否在拟选择区域内 -> 显示手型光标 (`PointingHandCursor`)。
  - 其他情况 -> 显示十字准星 (`CrossCursor`) 或标注工具光标。
