# Design — 禁用 overlay 窗口的 KWin 动画

## 背景

`src/overlay/mod.rs:152-154` 创建 wlr-layer-shell `LayerSurface` 时传入的 namespace（也就是 `zwlr_layer_shell_v1::get_layer_surface` 的 `namespace` 字段）是字符串 `"takeashot"`：

```rust
let layer = self.layer_shell.create_layer_surface(
    qh, surface, Layer::Overlay, Some("takeashot"), output,
);
```

KWin 在 `src/layershellv1window.cpp` 里通过 `scopeToType(scope)` 把 namespace 映射成 `NET::WindowType`：

| scope (namespace) | WindowType |
| --- | --- |
| `desktop` | `Desktop` |
| `dock` | `Dock` |
| `notification` | `Notification` |
| `on-screen-display` | `OnScreenDisplay` |
| `tooltip` | `Tooltip` |
| `dialog` | `Dialog` |
| `splash` | `Splash` |
| `utility` | `Utility` |
| 其他（含 `takeashot`） | `Normal` |

KWin 的 Scale / Fade 插件只在 `w.normalWindow || w.dialog` 的窗口上生效（见 `src/plugins/scale/package/contents/code/main.js` 的 `isScaleWindow`、`src/plugins/fade/package/contents/code/main.js` 的 `isFadeWindow`）。所以我们现在拿到的是 `Normal` 类型，正好命中动画。

## 方案：把 namespace 改成 `on-screen-display`

只改一个字符串：`"takeashot"` → `"on-screen-display"`。之后 KWin 会把我们的 layer-shell 当成 OSD，`normalWindow == false`，Scale/Fade 插件直接跳过；`sliding_popups` 在这种几乎占满整个输出的 overlay 上不会产生可见滑入。

### 为什么选 OSD 而不是 `notification` / `splash` / 其他

- `notification` 类型在 KWin 里会走 `sliding_popups` 滑入动画（KWin 把通知按 `Notification.svg` 动画）——反而会触发我们想避免的动画。
- `splash` 有 `ksplashqml` 的专用处理路径，不适合借用。
- `tooltip` / `utility` / `dialog` 在 Scale/Fade 插件里都是"被动画"的对象之一。
- `on-screen-display` 是 KDE 给音量/亮度指示器预留的类型，官方效果里绝大多数都显式跳过，语义上也匹配我们这个"冻结屏幕，等用户操作完就消失"的覆盖层。

### 对既有代码的影响

- 我们自己从不用 `namespace == "takeashot"` 做筛选（只有 `.desktop` 文件里的 X-KDE 字段、D-Bus service name 叫 `com.takeashot.service`），搜过没有地方依赖这个字符串。
- `keyboard_interactivity = Exclusive`、`anchor = TOP|BOTTOM|LEFT|RIGHT`、`exclusive_zone = -1` 都与 WindowType 无关，继续有效。
- KWin 对 OSD 类型窗口默认放进 `OnScreenDisplayLayer` 堆叠层；但因为我们显式声明 `Layer::Overlay`，堆叠仍然以 layer-shell 指定的为准——行为不变。

## 被排除的其他路线

### Spectacle 的 `xdg_toplevel_tag_v1("region-editor")`

Spectacle 在 `src/Gui/CaptureWindow.cpp:50` 用的是 `KWaylandExtras::setXdgToplevelTag(this, "region-editor")`，对应 KWin commit `5ad1949d`（2025-12）在 Scale/Fade/Glide 插件里硬编码：只有 `windowClass == "spectacle org.kde.spectacle" && tag == "region-editor"` 才跳过动画。原因：

1. 写死了 Spectacle 的 WM_CLASS；我们若想命中要把自己的 app-id 也改成这个，语义上不合适。
2. 只对 xdg-toplevel 生效；我们用的是 layer-shell，根本没有 `tag()`。
3. 需要 Plasma 6.4+，低版本无效。

### KWin 脚本 `window.skipsCloseAnimation = true`

只管 close，不管 open；且脚本有延迟，打开动画往往已经开始。只能作为补丁，不适合当主方案。

### 禁用整个 KWin 合成动画

影响全局，不是我们该动的范围。

## 风险与验证

- **风险**：理论上用户可能安装第三方 KWin 效果，对 OSD 类型窗口加了动画。这是用户自己的选择，非我们可控。
- **验证方式**：
  1. 开着默认 KWin 效果跑 `cargo run -- --smoke`，肉眼确认 overlay 不再有 scale-in。
  2. 临时把 namespace 改回 `takeashot`，对比动画有无，确认改动生效。
  3. 跑 `cargo test`，所有既有测试通过。
