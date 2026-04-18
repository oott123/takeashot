# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`takeashot` — KDE Plasma Wayland screenshot tool being rewritten in Rust + wgpu from an older Python/Qt5 implementation. The authoritative behavior spec is `project-overview.md` (Chinese); the migration/architecture rationale is in `plan.md`. Both are required reading before non-trivial work — the current `src/` tree is a partial implementation tracking that plan.

KDE Wayland only, by design: the tool relies on KWin's `ScreenShot2` D-Bus interface and KWin's Scripting API for window enumeration, which have no cross-desktop equivalents.

## Build / Run

```
cargo build
cargo run                    # normal: register tray/hotkey, wait for Pause
cargo run -- --now           # trigger a capture immediately after startup
cargo run -- --smoke         # show overlay for 3s and exit (no single-instance check)
cargo test                   # unit tests (selection state machine lives in src/selection.rs)
```

Toolchain is pinned via `mise.toml` (latest stable rust + rust-analyzer). A `flake.nix` is also provided.

### Runtime prerequisite: the `.desktop` file

KWin's `ScreenShot2` D-Bus interface is access-controlled. Calls will fail unless `resources/takeashot.desktop` is installed to `~/.local/share/applications/` — it declares `X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2`. If captures return permission errors, this is almost always the cause.

The Pause-key hotkey uses raw evdev (`/dev/input/event*`), so the running user must have read access to input devices (typically via the `input` group). This is intentional — Wayland's security model blocks global key listeners, so we bypass the compositor entirely.

## Architecture

The app is a single long-running process triggered by the Pause key or a second invocation. Each trigger starts one `ShotSession` that paints a frozen screenshot + dimmed overlay on every monitor, runs a selection/annotation loop, then copies the composed image to the clipboard.

### Process topology

- **`main.rs`** — parses args, connects to the session bus once (shared across all D-Bus work), creates the `App`, registers the single-instance D-Bus service, spawns the hotkey task, and runs the main loop.
- **`single_instance.rs`** — registering `com.takeashot.service` on the session bus is both the single-instance lock and the IPC surface. If registration fails, we call `activate()` on the existing instance and exit. The service also exposes `receive_window_data(String)` which is how KWin's scripting engine ships window lists back to us (see below). `SessionHandle` carries the `activate` watch-channel sender and an `Arc<Mutex<Option<Sender<String>>>>` slot for the currently-active session's window-data receiver.
- **`app.rs`** — owns a `watch::Receiver<bool>` that is pulsed by both the hotkey task and D-Bus `activate()`; on each pulse it starts a session via `overlay::run`.
- **`hotkey.rs`** — tokio task that opens every `/dev/input/event*` device whose name contains "keyboard", merges their streams, and sends on the trigger channel when it sees `KEY_PAUSE` down. Device disconnects are handled gracefully, not fatally.

### Screenshot session (the hot path)

A session is currently driven from `overlay/mod.rs` rather than a separate `session/` module — the `session/` and `annotation/` and `ui/` directories in `plan.md` are empty placeholders that later milestones will populate.

- **`kwin/screenshot.rs`** — zbus proxy for `org.kde.KWin.ScreenShot2`. Uses `CaptureScreen(name, {"native-resolution": true}, fd)` per output with a `memfd`-backed fd (NOT a pipe — screenshot data is tens of MB on multi-monitor HiDPI and pipes deadlock on full buffers). Metadata (`width`/`height`/`stride`/`format`) comes back in the reply dict; pixel data is BGRA read from the memfd. `CaptureWorkspace` is the fallback when per-screen capture fails.
- **`capture.rs`** — enumerates outputs and drives `screenshot.rs` in parallel, yielding `CapturedScreen` structs that carry BGRA pixels + geometry.
- **`kwin/windows.rs`** — placeholder for the KWin Scripting API pipeline (load JS → run → wait for `receive_window_data` callback with 5s timeout → unload). Not yet wired up.
- **`overlay/mod.rs`** — one `wlr-layer-shell` `LayerSurface` per output (layer=`Overlay`, anchors all four sides, keyboard interactivity exclusive). Uses smithay-client-toolkit directly (not winit — winit doesn't support layer-shell, which is why this rewrite dropped it). Each layer surface gets its own wgpu `Surface`; the `Gpu` in `overlay/renderer.rs` holds a shared `Instance`/`Adapter`/`Device`/`Queue` and all pipelines. Handles Wayland events (compositor/output/seat/keyboard/pointer/shm/layer) via SCTK's delegate macros.
- **`overlay/renderer.rs` + `overlay/shaders/*.wgsl`** — three pipelines: `screenshot.wgsl` blits the captured BGRA frame, `overlay.wgsl` applies the dim mask with the selection rect punched out, `handles.wgsl` draws the 8 resize handles and selection border.
- **`selection.rs`** — pure state machine (`SelectionState` with `None` / `Pending` / `Confirmed` variants, `CursorShape` output). This is the correctness-critical core; keep it UI-agnostic so it stays unit-testable. Rules for the 9-zone "click outside to expand" behavior, handle hit-testing, cursor mapping, and window-snapping come from `project-overview.md` sections 3–4 — changes here should be checked against that spec.
- **`geom.rs`** — `Rect`/`Point` helpers and conversions between global compositor coordinates and per-output local coordinates. Multi-monitor selection rendering relies on each `OutputOverlay` knowing its `output_pos` so it can render only the portion of the global selection that lives on its screen.

### Coordinate systems — watch out

Three coordinate spaces coexist and must not be mixed:

1. **Global logical** — the compositor's virtual desktop in logical pixels. Selection rects live here.
2. **Per-output logical** — `global - output_pos`. Used when handing geometry to a specific layer surface.
3. **Physical pixels** — logical × `scale_factor`. Capture buffers and wgpu surface configs use this; `native-resolution: true` on `CaptureScreen` is what makes HiDPI output sharp.

When final-composing the screenshot, each output's physical buffer is stitched by its global logical position × its own scale factor. Getting this wrong produces subtly misaligned multi-monitor output.

## Implementation status vs plan

`plan.md` describes an 8-milestone rewrite. As of the current tree: single-instance, hotkey, capture, and the overlay with a working selection state machine + wgpu rendering are in place. Toolbar (egui), annotations, window snapping, clipboard export, and tray are still TODO — their module directories exist but are empty. When adding these, follow the layout in `plan.md` rather than inventing a new structure.


这个项目由 python 项目改写而来，老项目在 .references/takeashot/ 里，但是不完全一样，搞不明白的细节可以看看
