# Takeashot 重写计划：Python/Qt5 → Rust/wgpu

## Context

原版 Takeashot 是 Python + Qt5/QML 实现的 KDE Wayland 截图工具。功能已在 `project-overview.md` 中完整固化（需求规格），但 Python/Qt 栈有几个问题：启动慢、分发需带 Qt 运行时、QML 与原生代码之间交互繁琐、全局按键和 D-Bus 脚本拼装方式分散在多处。

目标：用 Rust + wgpu 从零重写，保留 `project-overview.md` 描述的全部用户可见行为（含选区三态、窗口吸附、标注、工具栏、多屏 HiDPI、单实例、Pause 热键），同时获得更小的二进制、更快的启动、以及统一的 async/事件循环架构。仓库目前只有 `mise.toml` 和 `project-overview.md`，没有任何 Python 源码，相当于全新项目。

## 关键技术决策（已与用户对齐）

| 方面 | 选择 | 备注 |
|---|---|---|
| 渲染 | **wgpu** | 逐屏 surface；截图 BGRA 上传为纹理 |
| 窗口/合成 | **smithay-client-toolkit (SCTK) + wlr-layer-shell** | **替代 winit**——winit 不支持 layer-shell，无法在 KDE 下做真正的全屏 overlay。SCTK 直接说 Wayland 协议 |
| UI 工具栏 | **egui + egui-wgpu** | 取代 QML，声明式即时模式 UI |
| 剪贴板 | **wl-clipboard-rs** | 原生 `wl_data_device`，写 `image/png` MIME |
| 热键 | **evdev (tokio-evdev 或 evdev crate)** | 沿用原方案，读 `/dev/input/event*` |
| 托盘 | **ksni** | 原生 StatusNotifierItem via zbus |
| D-Bus | **zbus** (async) | KWin ScreenShot2、Scripting、自身服务全部用 zbus |
| 异步运行时 | **tokio** | zbus、evdev、截图捕获并行 |
| 策略 | 从零开始 | 无历史 Python 代码需保留 |

## 工程结构

单一 Cargo 工程 `takeashot/`，按关注点拆 crate-internal 模块（非多 crate workspace，除非某块明显可独立复用）：

```
takeashot/
├── Cargo.toml
├── resources/
│   ├── takeashot.desktop         # 注册 X-KDE-DBUS-Restricted-Interfaces
│   └── icons/                    # Tabler icons 子集（SVG）
├── src/
│   ├── main.rs                   # 入口：单实例检查 → 启动 tokio runtime
│   ├── app.rs                    # 顶层 App：组合 tray/hotkey/session
│   ├── single_instance.rs        # 自身 D-Bus 服务注册；失败则 activate() 已有实例
│   ├── tray.rs                   # ksni 托盘图标 + 菜单
│   ├── hotkey.rs                 # evdev Pause 监听（tokio task）
│   ├── kwin/
│   │   ├── mod.rs
│   │   ├── screenshot.rs         # ScreenShot2 D-Bus (CaptureScreen/CaptureWorkspace)
│   │   ├── windows.rs            # Scripting 加载 JS → 收窗口列表（5s 超时）
│   │   └── window_script.js      # 嵌入 include_str! 的 KWin 脚本
│   ├── capture.rs                # 并行抓每个显示器，解码 BGRA → image::RgbaImage
│   ├── session/                  # 一次截图会话的生命周期
│   │   ├── mod.rs                # ShotSession：创建遮罩 → 运行事件循环 → 输出或取消
│   │   ├── selection.rs          # 选区状态机（None / Pending / Confirmed）
│   │   ├── handles.rs            # 8 个手柄命中测试；外部扩展的方向推导
│   │   ├── snap.rs               # 窗口吸附（根据 stackingOrder 匹配鼠标）
│   │   └── cursors.rs            # 十字/移动/双向箭头的映射
│   ├── overlay/                  # 遮罩窗口（每屏一个）
│   │   ├── mod.rs                # SCTK LayerSurface 封装
│   │   ├── renderer.rs           # wgpu 设备/队列/管线（共享 adapter）
│   │   ├── pipelines.rs          # 背景图 + 暗色遮罩 + 选区挖空 + 标注
│   │   └── text.rs               # glyphon 或 wgpu_text（手柄附近坐标数字等）
│   ├── ui/                       # egui 工具栏叠层
│   │   ├── mod.rs                # egui-wgpu render pass；位置由 session 提供
│   │   └── toolbar.rs            # 工具按钮；定位策略（6.1）
│   ├── annotation/               # 标注模型 + 编辑
│   │   ├── mod.rs                # Shape 枚举：Pen/Line/Rect/Ellipse
│   │   ├── edit.rs               # 选中/移动/缩放/旋转/删除
│   │   └── render.rs             # 转成 wgpu 几何（lyon tessellate）
│   ├── compose.rs                # 确认时：截图 + 标注 → 最终 PNG
│   ├── clipboard.rs              # wl-clipboard-rs 写 image/png
│   └── geom.rs                   # Rect/Point/DPR helpers，跨屏坐标转换
└── tests/
    └── selection.rs              # 选区状态机的纯单元测试
```

## 模块落地要点

### 1. 启动与单实例 (`main.rs`, `single_instance.rs`)
- 进程启动即尝试在 session bus 注册 `com.takeashot.service`（zbus `ConnectionBuilder::serve_at`）。
- 注册失败 → 作为客户端调用已有实例的 `activate()`，然后 `std::process::exit(0)`。
- 注册成功 → 实现 `activate()`（触发一次 `ShotSession`）和 `receive_window_data(String)`（转发给当前 session 的 oneshot channel）。
- 注意：`receive_window_data` 的实现需要能访问"当前活跃 session"的句柄——用 `Arc<Mutex<Option<Sender<String>>>>` 即可。

### 2. 热键 (`hotkey.rs`)
- tokio task 扫描 `/dev/input/event*`，用 `evdev::Device::open` 打开名称含 `keyboard` 的设备。
- 对每个设备 `into_event_stream()`，`select_all` 合并。
- 遇到 `KEY_PAUSE` down → 发送 `SessionTrigger` 消息给 app 主循环。
- 设备错误 → 从流中移除，不崩溃（匹配原版行为）。

### 3. 截图捕获 (`kwin/screenshot.rs`, `capture.rs`)
- zbus proxy 接 `org.kde.KWin.ScreenShot2`。
- 对每个 Wayland output 并行 `CaptureScreen(name, {"native-resolution": true}, fd)`：
  - `memfd_create` 或 `tempfile::tempfile()` 拿到 `OwnedFd`。
  - 调用完成后 `mmap` 读 metadata 中的 `width/height/stride/format`。
  - BGRA → RGBA 转换（SIMD 或直接 swizzle）。
- 任一屏失败则整体回退到 `CaptureWorkspace` 并按几何切片。
- **需要**: 安装 `resources/takeashot.desktop` 到 `~/.local/share/applications/` 才能通过 KWin 权限检查；安装步骤放进 README / `cargo xtask install`。

### 4. 窗口列表 (`kwin/windows.rs`)
- `include_str!("window_script.js")` 嵌入脚本。运行时写入 `/tmp/takeashot-XXXX.js`。
- 脚本通过 `callDBus("com.takeashot.service", "/com/takeashot/Service", "com.takeashot.Service", "receive_window_data", JSON.stringify(list))` 回传。
- 流程：`loadScript` → `run` → 等待 oneshot（5s 超时） → `unloadScript` → 删临时文件。
- 超时/失败 → 返回空列表，静默禁用吸附。

### 5. 遮罩窗口 (`overlay/`)
- **每个 output 创建一个 SCTK `LayerSurface`**：layer=`Overlay`，anchor=四边，exclusive_zone=-1，keyboard_interactivity=`Exclusive`。
- 每个 layer surface 绑定一个 wgpu `Surface`（通过 `raw-window-handle` + `wgpu::SurfaceTargetUnsafe::from_window`，SCTK 的 `wl_surface` 提供 display/surface handle）。
- 共享一个 wgpu `Instance`/`Adapter`/`Device`/`Queue`。每屏独立 `SurfaceConfiguration`（物理像素尺寸）。
- 光标通过 wl-pointer 的 `set_cursor` 切换，形状用 `cursor-shape-v1` 协议（KDE 支持），或回退到传统 theme。

### 6. 选区状态机 (`session/selection.rs`)
```rust
enum Selection {
    None,
    Pending(Rect),          // 窗口吸附预览
    Confirmed { rect: Rect, handles: [Handle; 8] },
}
```
- 纯函数式转换：接受 `InputEvent + &WindowList` → 产生 `(Selection, Option<CursorShape>)`。
- 外部扩展方向的推导（3.3）：按点击点相对 rect 的九宫格位置决定扩展轴。
- 写单元测试覆盖所有 3.3、3.5 的边界情况——这是唯一与渲染解耦且容易回归的层。

### 7. 工具栏 (`ui/toolbar.rs`)
- egui-wgpu 的 render pass 作为 overlay 的最后一层。
- 定位策略 6.1 实现为纯函数 `place_toolbar(selection: Rect, screen: Rect, toolbar_size: Vec2) -> Pos2`。
- 事件穿透 (5.3)：egui 消费一个事件后打 `ctx.is_pointer_over_area()`；若 false → 让事件继续流向 selection 逻辑。这比 Qt 的事件掩码更干净。

### 8. 标注 (`annotation/`)
- Shape 枚举带 `transform: Affine2`（`glam`），编辑操作只改 transform 与原始几何。
- 用 `lyon` tessellate 成三角形带，提交给 wgpu 一个简单的带 MSAA 的 pipeline。
- 选中态的手柄/旋转柄直接用 egui 画（overlay 层），免自己实现命中测试 UI。
- 裁剪：片段着色器用选区 rect 做 `discard`，或用 scissor rect——scissor 更快。

### 9. 合成输出 (`compose.rs` → `clipboard.rs`)
- Enter/双击 → 对每屏离屏 render 到 `Texture`，readback 到 CPU，按几何拼到一张 `image::RgbaImage`。
- 编码 PNG (`image` crate) → `wl_clipboard_rs::copy::Source::Bytes` with MIME `image/png`。
- 写入后结束 session，销毁所有 layer surface。

### 10. 托盘 (`tray.rs`)
- `ksni::Tray` 实现：图标、菜单项（"截图 / 退出"），`activate()` 回调触发 session。
- 挂在 tokio task 里，失败时降级为无托盘运行（仍保留热键和单实例激活）。

## 关键文件（实施时重点）
- `src/session/selection.rs` — 状态机正确性决定整个 UX，优先写 + 测试
- `src/overlay/mod.rs` — SCTK layer-shell + wgpu 接线是最不熟的部分，优先原型验证
- `src/kwin/screenshot.rs` + `capture.rs` — 失败后全流程不可用，早做联通性验证
- `resources/takeashot.desktop` — 缺它 KWin 直接拒绝调用

## 可复用现有 crate 清单
- `wgpu` 25.x, `egui` + `egui-wgpu`
- `smithay-client-toolkit` (layer-shell, seat, output, xdg)
- `raw-window-handle` 0.6
- `wayland-protocols` (cursor-shape-v1)
- `wl-clipboard-rs`
- `zbus` (async, tokio feature)
- `ksni`
- `evdev` + tokio stream
- `image`, `lyon`, `glam`, `tempfile`, `serde_json`, `tokio`, `anyhow`, `tracing`

## 实施顺序（里程碑）
1. **M1 骨架**：Cargo 工程 + 单实例 D-Bus + Pause 热键 + 日志 → 能打印 "trigger" 即止。
2. **M2 截图可用**：ScreenShot2 + CaptureScreen 全屏抓 + PNG 落盘（先不上屏）。验证 `.desktop` 注册正确。
3. **M3 遮罩上屏**：SCTK layer-shell × wgpu，每屏显示 BGRA 截图 + 半透黑遮罩，Esc 退出。
4. **M4 选区状态机**：实现 3.1–3.3 + 光标，无吸附、无标注、无工具栏。单元测试先行。
5. **M5 工具栏 + 标注**：egui 工具栏；画笔/直线/矩形/椭圆；标注编辑工具。
6. **M6 窗口吸附**：KWin Scripting 流水线 + Pending 状态接入 selection。
7. **M7 合成 + 剪贴板**：确认流程 → 合成 PNG → wl-clipboard-rs。
8. **M8 托盘 + 打包**：ksni 托盘；`cargo xtask install` 安装 `.desktop` 与 autostart。

## 验证方式
- **端到端烟测**：`cargo run` → 按 Pause → 拖选 → 画标注 → Enter → `wl-paste -t image/png > out.png` 检查。
- **多屏**：在 KDE 系统设置里挂一个虚拟 output（或真双屏），验证跨屏选区拼合像素对齐。
- **HiDPI**：在 1.5× 缩放屏上验证输出图像是物理像素分辨率（对比截图文件尺寸 vs 屏幕逻辑尺寸）。
- **吸附回退**：临时把 `window_script.js` 改坏 → 5s 超时后仍能手动拖选。
- **单实例**：起两个进程，第二个应立刻退出且第一个进入截图态。
- **权限**：删掉 `~/.local/share/applications/takeashot.desktop` 重启，确认捕获被 KWin 拒并有清晰错误日志。
- **单元测试**：`cargo test -p takeashot selection::` 覆盖选区状态机与工具栏定位纯函数。

## 已知风险 / 待确认
- cursor-shape-v1 在某些 KWin 版本可能未启用，需要 fallback 到 `wl_pointer.set_cursor` + themed cursor。
- egui 的 DPI 与 Wayland fractional-scaling 的对齐需要在 M3 阶段验证。
