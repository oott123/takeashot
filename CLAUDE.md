# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`takeashot` — KDE Plasma Wayland screenshot tool being rewritten in Rust + wgpu from an older Python/Qt5 implementation. The authoritative behavior spec is `project-overview.md` (Chinese); the migration/architecture rationale is in `plan.md`. Both are required reading before non-trivial work.

KDE Wayland only, by design: the tool relies on KWin's `ScreenShot2` D-Bus interface and KWin's Scripting API for window enumeration, which have no cross-desktop equivalents.

The old Python project lives in `.references/takeashot/` — consult it when spec details are ambiguous, but note it's not an exact match.

## Build / Run

```
cargo build
cargo run                    # normal: register tray/hotkey, wait for Pause
cargo run -- --now           # trigger a capture immediately after startup
cargo run -- --smoke         # show overlay for 3s and exit (random D-Bus name, no single-instance check)
cargo test                   # all unit tests
cargo test -- selection      # selection state machine tests only
cargo test -- snap           # snap matching tests only
```

Toolchain is pinned via `mise.toml` (latest stable rust + rust-analyzer). A `flake.nix` is also provided.

### Runtime prerequisites

- **`.desktop` file**: KWin's `ScreenShot2` D-Bus interface is access-controlled. Install `resources/takeashot.desktop` to `~/.local/share/applications/` — it declares `X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2`. If captures return permission errors, this is almost always the cause.
- **Input group**: The Pause-key hotkey uses raw evdev (`/dev/input/event*`), so the running user must have read access to input devices (typically via the `input` group).
- **D-Bus service name**: Main mode registers `com.takeashot.service`. Smoke mode uses `com.takeashot.smoke.s{pid}` to avoid conflicts.

## Architecture

The app is a single long-running process triggered by the Pause key or a second invocation. Each trigger starts one screenshot session that paints a frozen screenshot + dimmed overlay on every monitor, runs a selection/annotation loop, then copies the composed image to the clipboard.

### Process topology

- **`main.rs`** — parses args, connects to the session bus (shared across all D-Bus work), dispatches to smoke mode or normal mode. Smoke mode registers a random D-Bus service name, fetches window list, then calls `overlay::run_with_timeout`.
- **`single_instance.rs`** — registering `com.takeashot.service` on the session bus is both the single-instance lock and the IPC surface. The service exposes `activate()` (trigger session) and `receive_window_data(String)` (KWin script callback). Smoke mode uses `register_smoke_service` with a custom name. D-Bus method names use `#[zbus(name = "...")]` attributes because zbus defaults to PascalCase but callers use snake_case.
- **`app.rs`** — owns a `watch::Receiver<bool>` pulsed by hotkey/D-Bus `activate()`. On each pulse, fetches the window list via `kwin::windows::fetch_window_list` (must happen in async context before the blocking overlay loop), then calls `overlay::run(dbus_conn, windows)`.
- **`hotkey.rs`** — tokio task scanning `/dev/input/event*` for keyboard devices, merging streams, sending on trigger channel for `KEY_PAUSE`. Device disconnects are handled gracefully.

### Screenshot session

- **`kwin/screenshot.rs`** — zbus calls to `org.kde.KWin.ScreenShot2`. `CaptureScreen` per output with `memfd`-backed fd (NOT a pipe — screenshot data is tens of MB and pipes deadlock). `CaptureWorkspace` as fallback.
- **`kwin/windows.rs`** — KWin Scripting pipeline: `loadScript` → `run` → wait for `receive_window_data` oneshot callback (5s timeout) → `unloadScript`. The JS script (`window_script.js`) uses `{{SERVICE_NAME}}` placeholder replaced at runtime. Window coordinates are `f64` (KWin returns floats); `WindowInfo` uses `#[serde(rename = "resourceClass")]` for camelCase JSON keys. Script file is written to CWD (not `/tmp`) because sandboxed environments may not share `/tmp` with KWin.
- **`capture.rs`** — enumerates outputs, drives `screenshot.rs` in parallel, yields `CapturedScreen` structs.
- **`snap.rs`** — pure function `find_snap_window(windows, pointer) → Option<Rect>`. Windows must be in front-to-back order (topmost first); the JS script reverses `stackingOrder` for this.
- **`overlay/mod.rs`** — one `wlr-layer-shell` `LayerSurface` per output (SCTK, not winit — winit doesn't support layer-shell). Each layer surface gets its own wgpu `Surface` with a shared `Gpu`. Handles Wayland events via SCTK delegate macros. `OverlayState` holds the window list for snap matching.
- **`overlay/renderer.rs` + `overlay/shaders/*.wgsl`** — shared GPU resources. `build_selection_vertices(rect, surface_size, include_handles)` draws border only (Pending/Creating) or border + 8 handles (Confirmed). Render pass order: screenshot → annotations → selection handles → egui toolbar.
- **`selection.rs`** — pure state machine with four `Selection` variants: `None`, `Pending` (snap preview), `Creating` (drag intermediate), `Confirmed`. `DragOp::PendingSnap` handles the click-vs-drag distinction on snap previews. Cancel semantics: Esc/right-click on `Pending` or `None` exits the overlay; on `Confirmed`/`Creating` clears selection. This is the correctness-critical core — keep it UI-agnostic and unit-testable. Changes should be checked against `project-overview.md` sections 3–4.
- **`geom.rs`** — `Rect`/`Point` helpers. Multi-monitor rendering relies on each `OutputOverlay` knowing its `output_pos` to convert global selection rects to per-output local coords.
- **`annotation/`** — `Shape` enum (Pen/Line/Rect/Ellipse) with `Affine2` transform. `render.rs` uses lyon tessellation into `ColoredVertex` wgpu buffers. Edit handles rendered via the same wgpu pipeline as selection handles, not egui.
- **`ui/toolbar.rs`** — egui-wgpu toolbar. `place_toolbar` is a pure function for positioning. Pointer ownership is locked on Press (toolbar vs overlay) until Release. Toolbar hit-testing uses self-computed geometry, not egui's `is_pointer_over_egui()`.

### Coordinate systems — watch out

Three coordinate spaces coexist and must not be mixed:

1. **Global logical** — the compositor's virtual desktop in logical pixels. Selection rects and window positions live here.
2. **Per-output logical** — `global - output_pos`. Used when handing geometry to a specific layer surface.
3. **Physical pixels** — logical × `scale_factor`. Capture buffers and wgpu surface configs use this; `native-resolution: true` on `CaptureScreen` is what makes HiDPI output sharp.

## Implementation status vs plan

`plan.md` describes an 8-milestone rewrite. M1–M6 are complete: single-instance, hotkey, capture, overlay with selection state machine + wgpu rendering, toolbar + annotations, and window snapping. Remaining: M7 (compose + clipboard), M8 (tray + packaging). When adding these, follow the layout in `plan.md` rather than inventing a new structure.
