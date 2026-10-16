# Capto Architecture

## Goals

- Lightweight, local-only Captura successor
- Windows 10 1903+ first; cross-platform via `CaptureBackend`
- Smooth UI: Tauri 2 + React; heavy work in Rust

## Recording pipeline

```
CaptureBackend  →  OverlayCompositor  →  Encoder (FFmpeg sidecar)
AudioBackend    ──────────────────────↗
Webcam (PiP)    ─↗
```

1. **Capture** produces frames (timestamp + pixel buffer) or, for the MVP path, FFmpeg grabs the desktop directly while overlay filters are applied via `filter_complex` when needed.
2. **OverlayCompositor** stacks click ripples, keystrokes, text/image overlays, webcam PiP (elapsed timer is UI-only, not burned in).
3. **Audio** on Windows is captured natively by `capto-audio` through WASAPI
   (capture endpoint for mic, render endpoint with loopback mode for system
   sound). Each source is normalized to 48 kHz stereo `f32le`, paced against
   wall-clock time with silence insertion, and streamed to `capto-encode` over
   localhost TCP. FFmpeg mixes and muxes those PCM inputs; it does not access
   Windows audio devices directly.
4. **Encoder** prefers `h264_nvenc` → `h264_qsv` → `h264_amf` → `libx264`. GIF uses palettegen/paletteuse.

## Traits

### `CaptureBackend` (`capto-capture`)

```rust
fn list_displays(&self) -> Result<Vec<DisplayInfo>>;
fn list_windows(&self) -> Result<Vec<WindowInfo>>;
fn capture_frame(&self, target: &CaptureTarget) -> Result<Frame>;
fn platform_name(&self) -> &'static str;
```

- **Windows**: WGC-oriented implementation (`WindowsCaptureBackend`), screenshots via `xcap`.
- **macOS / Linux**: `UnsupportedCaptureBackend` until ScreenCaptureKit / PipeWire land.

### `capto-encode::FfmpegEncoder`

- Resolves binary: **bundled sidecar only** (`binaries/ffmpeg` / next to the app exe). Never system `PATH`.
- `probe_encoders()` lists hardware + software codecs
- Builds argv for MP4 / GIF / audio-only

## Session state machine (`capto-core`)

`Idle → Starting → Recording ⇄ Paused → Stopping → Idle`  
Errors transition to `Idle` with last error message.

## UI contract

Tauri commands (intent only):

- `get_settings` / `save_settings`
- `list_sources` / `list_audio_devices` / `list_encoders` / `list_webcams`
- `start_recording` / `pause_recording` / `resume_recording` / `stop_recording`
- `take_screenshot`
- `get_session_state`

Events: `session://state`, `session://tick`, `settings://changed`

## Single instance

Desktop is **single-process only** via `tauri-plugin-single-instance` (registered first). A second launch exits immediately and focuses the existing `main` window. There is one `RecordingSession` and one CLI control plane for the whole machine.

## CLI control plane

The **`capto`** CLI (crate `capto-cli`) does **not** own a recording session. It talks to the running desktop (`capto-app`) over localhost HTTP:

1. Desktop binds `127.0.0.1:<ephemeral>` and writes `{config_dir}/Capto/cli-server.json` (`pid`, `port`, `token`, `version`).
2. CLI reads the lock file, sends `Authorization: Bearer <token>`, calls `/v1/...`.
3. If the plane is down, CLI spawns the desktop (`CAPTO_APP_PATH` / `capto-app.exe` / installed Capto) and polls until ready. Single-instance ensures that spawn cannot create a second session.

CLI vs desktop binaries stay distinct (`capto` vs `capto-app`) so they do not overwrite each other in `target/debug` or collide on case-insensitive Windows paths. The installer ships the CLI at `<install>/cli/capto.exe` and adds that folder to user PATH (not a separate Release asset).

Shared types live in `capto-ipc`. Default CLI stdout is a JSON envelope `{ ok, data | error }` with stable exit codes.

**Agent / command reference:** [CLI.md](CLI.md)  
**npm Agent Skill:** [`capto-agent-skill`](../packages/capto-agent-skill) (`skills/capto`)

Endpoints (v1): `status`, `doctor`, `config` (GET/PATCH), `config/path`, `record/start|stop|pause|resume`, `shot`, `list/{displays,windows,audio,encoders}`, `outputs/recent`, `outputs/open`.

## Security / privacy

- No network upload features in product code
- Output paths are user-controlled local directories
- Global hooks only while enabled in settings
- CLI control plane listens on loopback only; token in the user config dir

## Cross-platform

See [CROSS_PLATFORM.md](CROSS_PLATFORM.md) for macOS / Linux recording backend roadmap. Frame capture already uses `xcap` on all major desktop OSes.

## CI / Release

See [CI.md](CI.md) for GitHub Actions (CI vs Release, x64 + ARM64, FFmpeg pin from `ciryanna/capto-ffmpeg`).
