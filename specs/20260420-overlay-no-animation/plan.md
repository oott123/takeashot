# 执行计划 — 禁用 overlay 窗口的 KWin 动画

## 步骤 1：改 namespace

**文件**：`src/overlay/mod.rs`，第 152-154 行。

**改动**：

```rust
// 改前
let layer = self.layer_shell.create_layer_surface(
    qh, surface, Layer::Overlay, Some("takeashot"), output,
);

// 改后
let layer = self.layer_shell.create_layer_surface(
    qh, surface, Layer::Overlay, Some("on-screen-display"), output,
);
```

就这一行字符串。

## 步骤 2：本地编译 & 冒烟

```
cargo build
cargo run -- --smoke
```

肉眼确认：

- overlay 出现时没有肉眼可见的 scale-in / fade-in。
- overlay 退出时也尽量没有 scale-out / fade-out。
- 3 秒后正常自动退出，无错误日志。

## 步骤 3：跑测试

```
cargo test
```

确保既有单元测试仍然全部通过。`selection` / `snap` 那几套都和 overlay 生命周期无关，应该不受影响。

## 步骤 4：手动对照（可选）

如果第 2 步的观察不确定，可以在本地做一次对照实验：

1. 临时把 namespace 再改回 `"takeashot"`，`cargo run -- --smoke`，留意动画。
2. 再改回 `"on-screen-display"`，同样跑一次，对照差异。
3. 把观察写进 PR 说明里。

## 步骤 5：确认无其他引用

```
rg '"takeashot"' src/
```

只应该匹配到我们刚动过的那一行的上下文（没有其他地方把 `"takeashot"` 当 namespace 用）。

## 不做的事

- 不动 `.desktop` 文件，不动 D-Bus service name（`com.takeashot.service`），不动单例锁，不动进程名——这些都不是 layer-shell 的 namespace。
- 不引入 KWin 脚本、不引入窗口规则。
- 不做向下兼容 fallback：所有 Plasma/KWin 版本都支持 `scopeToType` 的这条映射，没有兼容性顾虑。
