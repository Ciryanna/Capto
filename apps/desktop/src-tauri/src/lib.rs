use capto_capture::CaptureTarget;
use capto_core::{
    AppSettings, OutputFormat, RecordingSession, Region, SessionSnapshot, SessionState,
    VideoSourceKind,
};
use capto_encode::{EncoderInfo, VideoEncoderKind};
use capto_hooks::HotkeyAction;
use capto_ipc::{RecordStartRequest, ShotRequest};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, PhysicalPosition, PhysicalSize, RunEvent, State, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tokio::sync::Mutex;

mod cli_server;
mod crashlog;
mod record_overlay;
mod session_svc;

/// Bumped each time selection overlays are opened so window labels stay unique
/// (`region-picker-3-0`). Reusing `region-picker-0` after close fails on Windows.
static OVERLAY_GENERATION: AtomicU64 = AtomicU64::new(1);

pub struct AppState {
    pub session: Mutex<RecordingSession>,
    pub overlay: Mutex<Option<record_overlay::RecordOverlayController>>,
    pub audio_meter: Mutex<Option<capto_audio::AudioMeterSession>>,
    /// Shortcuts Windows (or another app) would not let us register.  Keep
    /// this separate from saving settings so one unavailable binding never
    /// prevents the remaining bindings from being applied.
    pub hotkey_conflicts: std::sync::Mutex<Vec<String>>,
    /// Local, privacy-first metrics registry (served at /v1/metrics).
    pub metrics: capto_core::metrics::Metrics,
}

fn sidecar_dir(app: &AppHandle) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // Dev/build: always prefer the src-tauri/binaries next to this crate.
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries"));

    if let Ok(resource) = app.path().resource_dir() {
        candidates.push(resource.join("binaries"));
        candidates.push(resource.clone());
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            // Installed/portable: Tauri externalBin lands next to the app exe.
            candidates.push(parent.to_path_buf());
            candidates.push(parent.join("binaries"));
            // target/debug -> ../../apps/desktop/src-tauri/binaries (workspace layout)
            candidates.push(parent.join("../../apps/desktop/src-tauri/binaries"));
        }
    }

    candidates
        .into_iter()
        .find(|dir| capto_encode::FfmpegEncoder::dir_has_ffmpeg(dir))
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(session_svc::get_settings(&state).await)
}

#[tauri::command]
async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<(), String> {
    session_svc::save_settings(&app, &state, settings, register_hotkeys).await
}

#[tauri::command]
fn get_hotkey_conflicts(state: State<'_, AppState>) -> Vec<String> {
    state
        .hotkey_conflicts
        .lock()
        .map(|conflicts| conflicts.clone())
        .unwrap_or_default()
}

#[tauri::command]
fn default_output_dir() -> String {
    AppSettings::default().output_dir
}

#[tauri::command]
async fn list_displays(
    state: State<'_, AppState>,
) -> Result<Vec<capto_capture::DisplayInfo>, String> {
    let session = state.session.lock().await;
    session.capture().list_displays().map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_windows(
    state: State<'_, AppState>,
) -> Result<Vec<capto_capture::WindowInfo>, String> {
    let session = state.session.lock().await;
    session.capture().list_windows().map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_audio_devices(
    _state: State<'_, AppState>,
) -> Result<Vec<capto_audio::AudioDeviceInfo>, String> {
    capto_audio::list_devices().map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_webcams() -> Result<Vec<capto_capture::WebcamInfo>, String> {
    capto_capture::list_webcams().map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_encoders(state: State<'_, AppState>) -> Result<Vec<EncoderInfo>, String> {
    let mut session = state.session.lock().await;
    session.refresh_encoder().map_err(|e| e.to_string())?;
    let enc = session.encoder().ok_or_else(|| {
        "Bundled FFmpeg not found. Dev: run scripts/download-ffmpeg.ps1. Release: reinstall Capto.".to_string()
    })?;
    enc.probe_encoders().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_session_state(state: State<'_, AppState>) -> Result<SessionSnapshot, String> {
    let session = state.session.lock().await;
    Ok(session.snapshot().await)
}

#[tauri::command]
async fn get_audio_levels(state: State<'_, AppState>) -> Result<capto_audio::AudioLevels, String> {
    let session = state.session.lock().await;
    let levels = session.audio_levels();
    drop(session);
    if levels.microphone > 0.0 || levels.system > 0.0 {
        return Ok(levels);
    }
    Ok(state
        .audio_meter
        .lock()
        .await
        .as_ref()
        .map(|meter| meter.levels())
        .unwrap_or_default())
}

#[tauri::command]
async fn start_audio_meter(
    state: State<'_, AppState>,
    mic_device: Option<String>,
    loopback_device: Option<String>,
) -> Result<(), String> {
    if state.session.lock().await.snapshot().await.state != SessionState::Idle {
        return Err("audio test is unavailable while recording".into());
    }
    let meter =
        capto_audio::AudioMeterSession::start(mic_device.as_deref(), loopback_device.as_deref())
            .map_err(|e| e.to_string())?;
    *state.audio_meter.lock().await = meter;
    Ok(())
}

#[tauri::command]
async fn stop_audio_meter(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(meter) = state.audio_meter.lock().await.take() {
        meter.stop();
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartArgs {
    source: VideoSourceKind,
    display_id: Option<u32>,
    window_id: Option<u32>,
    region: Option<Region>,
    include_cursor: Option<bool>,
    mic_device: Option<String>,
    loopback_device: Option<String>,
    mic_volume: Option<u8>,
    loopback_volume: Option<u8>,
    encoder: Option<VideoEncoderKind>,
    format: Option<OutputFormat>,
    fps: Option<u32>,
    quality: Option<u8>,
}

impl From<StartArgs> for RecordStartRequest {
    fn from(args: StartArgs) -> Self {
        RecordStartRequest {
            source: args.source,
            display_id: args.display_id,
            window_id: args.window_id,
            region: args.region,
            include_cursor: args.include_cursor,
            mic_device: args.mic_device,
            loopback_device: args.loopback_device,
            mic_volume: args.mic_volume,
            loopback_volume: args.loopback_volume,
            encoder: args.encoder,
            format: args.format,
            fps: args.fps,
            quality: args.quality,
        }
    }
}

#[tauri::command]
async fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    args: StartArgs,
) -> Result<SessionSnapshot, String> {
    session_svc::start_recording(&app, &state, args.into()).await
}

#[tauri::command]
async fn pause_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SessionSnapshot, String> {
    session_svc::pause_recording(&app, &state).await
}

#[tauri::command]
async fn resume_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SessionSnapshot, String> {
    session_svc::resume_recording(&app, &state).await
}

#[tauri::command]
async fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SessionSnapshot, String> {
    session_svc::stop_recording(&app, &state).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShotArgs {
    source: VideoSourceKind,
    display_id: Option<u32>,
    window_id: Option<u32>,
    region: Option<Region>,
}

impl From<ShotArgs> for ShotRequest {
    fn from(args: ShotArgs) -> Self {
        ShotRequest {
            source: args.source,
            display_id: args.display_id,
            window_id: args.window_id,
            region: args.region,
        }
    }
}

fn capture_target(args: ShotArgs) -> Result<CaptureTarget, String> {
    session_svc::capture_target(&args.into())
}

#[tauri::command]
async fn take_screenshot(state: State<'_, AppState>, args: ShotArgs) -> Result<String, String> {
    session_svc::take_screenshot(&state, args.into()).await
}

/// Masked area as a 0..1 fraction of the frame, so the UI can badge it at any
/// preview scale.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MaskRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewFrame {
    width: u32,
    height: u32,
    /// Native capture size (before JPEG downscale). PiP sizes are in this space.
    source_width: u32,
    source_height: u32,
    jpeg: Vec<u8>,
    timestamp_ms: u64,
    app_masked: bool,
    mask_rect: Option<MaskRect>,
}

/// Capture one low-resolution preview frame. Uses DXGI Desktop Duplication on
/// Windows so the system cursor is not hidden/flickered by GDI BitBlt.
/// Frame processing stays native: React only receives the finished JPEG.
#[tauri::command]
async fn capture_preview(
    app: AppHandle,
    _state: State<'_, AppState>,
    args: ShotArgs,
) -> Result<PreviewFrame, String> {
    let app_rect = app.get_webview_window("main").and_then(|window| {
        if !window.is_visible().ok().unwrap_or(false) {
            return None;
        }
        let pos = window.outer_position().ok()?;
        let size = window.outer_size().ok()?;
        Some((pos.x, pos.y, size.width, size.height))
    });

    let (mut frame, origin) = match args.source {
        VideoSourceKind::Window => {
            let id = args
                .window_id
                .ok_or_else(|| "windowId required".to_string())?;
            let target = CaptureTarget::Window { id };
            capto_capture::capture_preview_frame(&target).map_err(|e| e.to_string())?
        }
        _ => {
            let target = capture_target(args)?;
            capto_capture::capture_preview_frame(&target).map_err(|e| e.to_string())?
        }
    };
    let timestamp_ms = frame.timestamp_ms;
    let (source_width, source_height) = (frame.width.max(1), frame.height.max(1));
    let (frame_w, frame_h) = (source_width as f32, source_height as f32);
    let painted = if let (Some((app_x, app_y, app_w, app_h)), Some((origin_x, origin_y))) =
        (app_rect, origin)
    {
        frame.blackout_rect(app_x - origin_x, app_y - origin_y, app_w, app_h)
    } else {
        None
    };
    let mask_rect = painted.map(|r| MaskRect {
        x: r.x as f32 / frame_w,
        y: r.y as f32 / frame_h,
        width: r.width as f32 / frame_w,
        height: r.height as f32 / frame_h,
    });

    // Main preview marks PiP with a UI icon; live cam preview lives on the Webcam tab.
    let (width, height, jpeg) = frame.preview_jpeg(480, 55).map_err(|e| e.to_string())?;

    Ok(PreviewFrame {
        width,
        height,
        source_width,
        source_height,
        jpeg,
        timestamp_ms,
        app_masked: mask_rect.is_some(),
        mask_rect,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebcamSoloFrame {
    width: u32,
    height: u32,
    jpeg: Vec<u8>,
    timestamp_ms: u64,
}

/// Dedicated webcam preview for the Webcam settings tab (MF, not getUserMedia).
#[tauri::command]
async fn capture_webcam_preview(
    state: State<'_, AppState>,
    device_id: Option<String>,
) -> Result<WebcamSoloFrame, String> {
    let session = state.session.lock().await;
    let snap = session.snapshot().await;
    if snap.state != SessionState::Idle {
        return Err("webcam preview paused while recording".into());
    }
    let cam = &session.settings().overlays.webcam;
    let tw = cam.width.max(160).min(640);
    let th = cam.height.max(120).min(480);
    let device = device_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(cam.device_id.as_deref())
        .or(cam.device_label.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let slot = capto_capture::ensure_preview_webcam(device, tw, th).map_err(|e| e.to_string())?;
    let mut frame = slot
        .latest()
        .ok_or_else(|| "webcam frame not ready yet".to_string())?
        .as_ref()
        .clone();
    // MF stores BGRA in Frame.rgba; JPEG encoder expects RGBA.
    capto_capture::swap_rb_inplace(&mut frame);
    if cam.mirrored {
        mirror_rgba_inplace(&mut frame);
    }
    let (width, height, jpeg) = frame.preview_jpeg(480, 72).map_err(|e| e.to_string())?;
    Ok(WebcamSoloFrame {
        width,
        height,
        jpeg,
        timestamp_ms: frame.timestamp_ms,
    })
}

#[tauri::command]
async fn release_preview_webcam() -> Result<(), String> {
    capto_capture::release_preview_webcam();
    Ok(())
}

#[tauri::command]
async fn release_preview_session() -> Result<(), String> {
    capto_capture::release_preview_session();
    Ok(())
}

fn mirror_rgba_inplace(frame: &mut capto_capture::Frame) {
    let w = frame.width as usize;
    let h = frame.height as usize;
    if w < 2 || frame.rgba.len() < w * h * 4 {
        return;
    }
    for y in 0..h {
        let row = y * w * 4;
        for x in 0..(w / 2) {
            let left = row + x * 4;
            let right = row + (w - 1 - x) * 4;
            for c in 0..4 {
                frame.rgba.swap(left + c, right + c);
            }
        }
    }
}

#[tauri::command]
async fn window_under_cursor() -> Result<Option<capto_capture::WindowInfo>, String> {
    let pid = std::process::id();
    capto_capture::window_under_cursor(Some(pid)).map_err(|e| e.to_string())
}

#[tauri::command]
async fn cursor_position() -> Result<capto_capture::ScreenPoint, String> {
    capto_capture::cursor_position().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_virtual_screen() -> Result<capto_capture::VirtualScreen, String> {
    Ok(capto_capture::virtual_screen())
}

async fn close_overlay_kind(app: &AppHandle, kind: &str) {
    let prefix = format!("{kind}-");
    let labels: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|l| *l == kind || l.starts_with(&prefix))
        .cloned()
        .collect();
    for label in labels {
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.close();
        }
    }
}

fn selection_overlay_open(app: &AppHandle) -> bool {
    app.webview_windows().keys().any(|l| {
        l == "picker"
            || l == "region-picker"
            || l.starts_with("picker-")
            || l.starts_with("region-picker-")
    })
}

/// Close every selection overlay (window + region). Leftover always-on-top
/// pickers after a region select were blocking the next window pick.
async fn close_all_selection_overlays(app: &AppHandle) {
    close_overlay_kind(app, "picker").await;
    close_overlay_kind(app, "region-picker").await;
    // Wait until labels are actually gone (destroy is async on Windows).
    for _ in 0..25 {
        if !selection_overlay_open(app) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        close_overlay_kind(app, "picker").await;
        close_overlay_kind(app, "region-picker").await;
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.set_focus();
    }
}

/// One transparent overlay per physical monitor.
/// A single window spanning the virtual desktop breaks on secondary / mixed-DPI screens.
async fn open_overlay_windows(app: &AppHandle, kind: &str, title: &str) -> Result<(), String> {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }

    let opened = async {
        // Always tear down both picker kinds before creating a new set.
        close_all_selection_overlays(app).await;

        let mut monitors = capto_capture::list_monitor_rects();
        if monitors.is_empty() {
            monitors.push(capto_capture::virtual_screen());
        }

        let gen = OVERLAY_GENERATION.fetch_add(1, Ordering::Relaxed);
        let mut created: Vec<String> = Vec::new();

        for (i, mon) in monitors.iter().enumerate() {
            let label = format!("{kind}-{gen}-{i}");
            let window = tauri::WebviewWindowBuilder::new(
                app,
                &label,
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title(title)
            .transparent(true)
            .decorations(false)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(true)
            .focused(i == 0)
            .background_color(tauri::window::Color(0, 0, 0, 0))
            .build()
            .map_err(|e| format!("create overlay {label}: {e}"))?;

            created.push(label.clone());

            window
                .set_position(tauri::Position::Physical(PhysicalPosition::new(
                    mon.x, mon.y,
                )))
                .map_err(|e| format!("position overlay {label}: {e}"))?;
            window
                .set_size(tauri::Size::Physical(PhysicalSize::new(
                    mon.width.max(2),
                    mon.height.max(2),
                )))
                .map_err(|e| format!("size overlay {label}: {e}"))?;
            let _ = window.set_fullscreen(false);
        }

        // Focus the overlay under the cursor when possible.
        if let Ok(pt) = capto_capture::cursor_position() {
            for (i, mon) in monitors.iter().enumerate() {
                if mon.contains_point(pt.x, pt.y) {
                    if let Some(w) = app.get_webview_window(&format!("{kind}-{gen}-{i}")) {
                        let _ = w.set_focus();
                    }
                    break;
                }
            }
        }

        let _ = created;
        Ok::<(), String>(())
    }
    .await;

    if opened.is_err() {
        close_all_selection_overlays(app).await;
        show_main_window(app);
    }
    opened
}

#[tauri::command]
async fn open_window_picker(app: AppHandle) -> Result<(), String> {
    open_overlay_windows(&app, "picker", "Capto — Pick Window").await
}

#[tauri::command]
async fn close_window_picker(app: AppHandle) -> Result<(), String> {
    close_all_selection_overlays(&app).await;
    show_main_window(&app);
    Ok(())
}

#[tauri::command]
async fn open_region_picker(app: AppHandle) -> Result<(), String> {
    open_overlay_windows(&app, "region-picker", "Capto — Select Region").await
}

#[tauri::command]
async fn close_region_picker(app: AppHandle) -> Result<(), String> {
    close_all_selection_overlays(&app).await;
    show_main_window(&app);
    Ok(())
}

#[tauri::command]
fn get_window_label(window: tauri::WebviewWindow) -> String {
    window.label().to_string()
}

#[tauri::command]
fn get_overlay_defaults() -> capto_overlay::OverlayConfig {
    capto_overlay::OverlayConfig::default()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlatformInfo {
    capture_backend: String,
    os: String,
    ffmpeg_path: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FfmpegInfo {
    available: bool,
    /// Capto FFmpeg release tag (e.g. `v1.0.0-n9.0`).
    bundle_version: Option<String>,
    /// Short FFmpeg version token from `ffmpeg -version`.
    ffmpeg_version: Option<String>,
    /// Full first line of `ffmpeg -version` (tooltip / detail).
    ffmpeg_version_line: Option<String>,
    path: Option<String>,
    /// GitHub repo slug, e.g. `ciryanna/capto-ffmpeg`.
    repository: Option<String>,
}

#[derive(Deserialize)]
struct CaptoFfmpegMeta {
    repository: Option<String>,
    tag: Option<String>,
}

fn pinned_capto_ffmpeg_meta() -> CaptoFfmpegMeta {
    const PIN: &str = include_str!("../../../../.github/capto-ffmpeg.env");
    let mut repository = None;
    let mut tag = None;
    for line in PIN.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        match k.trim() {
            "CAPTO_FFMPEG_REPO" => repository = Some(v.trim().to_string()),
            "CAPTO_FFMPEG_TAG" => tag = Some(v.trim().to_string()),
            _ => {}
        }
    }
    CaptoFfmpegMeta { repository, tag }
}

fn read_capto_ffmpeg_meta_near(ffmpeg_path: &std::path::Path) -> Option<CaptoFfmpegMeta> {
    let dir = ffmpeg_path.parent()?;
    let candidates = [
        dir.join("capto-ffmpeg.json"),
        dir.join("binaries").join("capto-ffmpeg.json"),
    ];
    for path in candidates {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(meta) = serde_json::from_str::<CaptoFfmpegMeta>(&text) {
                return Some(meta);
            }
        }
    }
    None
}

/// Strip Win32 extended/verbatim prefixes (`\\?\`, `\\.\`) for UI display.
fn display_path(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    let trimmed = s
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .or_else(|| s.strip_prefix(r"\\?\").map(|rest| rest.to_string()))
        .or_else(|| s.strip_prefix(r"\\.\").map(|rest| rest.to_string()))
        .unwrap_or_else(|| s.into_owned());
    trimmed
}

#[tauri::command]
async fn get_ffmpeg_info(app: AppHandle, state: State<'_, AppState>) -> Result<FfmpegInfo, String> {
    let encoder = {
        let mut session = state.session.lock().await;
        let _ = session.refresh_encoder();
        session.encoder().cloned()
    };
    let pinned = pinned_capto_ffmpeg_meta();
    let resource_meta = app.path().resource_dir().ok().and_then(|dir| {
        let path = dir.join("capto-ffmpeg.json");
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str::<CaptoFfmpegMeta>(&t).ok())
    });

    let Some(encoder) = encoder else {
        let meta = resource_meta.unwrap_or(pinned);
        return Ok(FfmpegInfo {
            available: false,
            bundle_version: meta.tag,
            ffmpeg_version: None,
            ffmpeg_version_line: None,
            path: None,
            repository: meta.repository,
        });
    };

    let path = display_path(encoder.binary_path());
    let meta = read_capto_ffmpeg_meta_near(encoder.binary_path())
        .or(resource_meta)
        .unwrap_or(pinned);
    match encoder.version_line().await {
        Ok(line) => Ok(FfmpegInfo {
            available: true,
            bundle_version: meta.tag,
            ffmpeg_version: capto_encode::parse_ffmpeg_version_token(&line)
                .or_else(|| Some(line.clone())),
            ffmpeg_version_line: Some(line),
            path: Some(path),
            repository: meta.repository,
        }),
        Err(e) => Ok(FfmpegInfo {
            available: true,
            bundle_version: meta.tag,
            ffmpeg_version: Some(e.to_string()),
            ffmpeg_version_line: None,
            path: Some(path),
            repository: meta.repository,
        }),
    }
}

#[tauri::command]
async fn open_output_folder(state: State<'_, AppState>) -> Result<String, String> {
    let session = state.session.lock().await;
    let dir = session.output_dir().to_string();
    drop(session);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

#[tauri::command]
async fn get_platform_info(state: State<'_, AppState>) -> Result<PlatformInfo, String> {
    let mut session = state.session.lock().await;
    let _ = session.refresh_encoder();
    let ffmpeg_path = session.encoder().map(|e| display_path(e.binary_path()));
    Ok(PlatformInfo {
        capture_backend: session.capture().platform_name().into(),
        os: std::env::consts::OS.into(),
        ffmpeg_path,
    })
}

async fn hotkey_start(app: &AppHandle) {
    let state = app.state::<AppState>();
    let session = state.session.lock().await;
    let snap = session.snapshot().await;
    let settings = session.settings().clone();
    drop(session);
    match snap.state {
        SessionState::Idle => {
            let _ = session_svc::start_recording(
                app,
                &app.state::<AppState>(),
                session_svc::default_start_from_settings(&settings),
            )
            .await;
        }
        SessionState::Paused => {
            let _ = resume_recording(app.clone(), app.state::<AppState>()).await;
        }
        _ => {}
    }
}

async fn hotkey_pause(app: &AppHandle) {
    let state = app.state::<AppState>();
    let snap = state.session.lock().await.snapshot().await;
    match snap.state {
        SessionState::Recording => {
            let _ = pause_recording(app.clone(), app.state::<AppState>()).await;
        }
        SessionState::Paused => {
            let _ = resume_recording(app.clone(), app.state::<AppState>()).await;
        }
        _ => {}
    }
}

async fn hotkey_stop(app: &AppHandle) {
    let state = app.state::<AppState>();
    let snap = state.session.lock().await.snapshot().await;
    if matches!(
        snap.state,
        SessionState::Recording | SessionState::Paused | SessionState::Starting
    ) {
        let _ = stop_recording(app.clone(), app.state::<AppState>()).await;
    }
}

async fn hotkey_screenshot(app: &AppHandle) {
    let state = app.state::<AppState>();
    let settings = state.session.lock().await.settings().clone();
    let _ = take_screenshot(
        app.state::<AppState>(),
        ShotArgs {
            source: settings.default_source,
            display_id: settings.default_display_id.or(Some(0)),
            window_id: None,
            region: settings.default_region,
        },
    )
    .await;
}

fn parse_hotkey_shortcut(s: &str) -> Option<Shortcut> {
    let mut mods = Modifiers::empty();
    let mut key: Option<Code> = None;
    for part in s.split('+') {
        let p = part.trim();
        match p {
            "CommandOrControl" | "Control" | "Ctrl" | "CmdOrCtrl" => {
                mods |= Modifiers::CONTROL;
            }
            "Shift" => mods |= Modifiers::SHIFT,
            "Alt" | "Option" => mods |= Modifiers::ALT,
            "Super" | "Meta" | "Command" => mods |= Modifiers::SUPER,
            other => {
                key = match other.to_ascii_uppercase().as_str() {
                    "A" => Some(Code::KeyA),
                    "B" => Some(Code::KeyB),
                    "C" => Some(Code::KeyC),
                    "D" => Some(Code::KeyD),
                    "E" => Some(Code::KeyE),
                    "F" => Some(Code::KeyF),
                    "G" => Some(Code::KeyG),
                    "H" => Some(Code::KeyH),
                    "I" => Some(Code::KeyI),
                    "J" => Some(Code::KeyJ),
                    "K" => Some(Code::KeyK),
                    "L" => Some(Code::KeyL),
                    "M" => Some(Code::KeyM),
                    "N" => Some(Code::KeyN),
                    "O" => Some(Code::KeyO),
                    "P" => Some(Code::KeyP),
                    "Q" => Some(Code::KeyQ),
                    "R" => Some(Code::KeyR),
                    "S" => Some(Code::KeyS),
                    "T" => Some(Code::KeyT),
                    "U" => Some(Code::KeyU),
                    "V" => Some(Code::KeyV),
                    "W" => Some(Code::KeyW),
                    "X" => Some(Code::KeyX),
                    "Y" => Some(Code::KeyY),
                    "Z" => Some(Code::KeyZ),
                    "0" => Some(Code::Digit0),
                    "1" => Some(Code::Digit1),
                    "2" => Some(Code::Digit2),
                    "3" => Some(Code::Digit3),
                    "4" => Some(Code::Digit4),
                    "5" => Some(Code::Digit5),
                    "6" => Some(Code::Digit6),
                    "7" => Some(Code::Digit7),
                    "8" => Some(Code::Digit8),
                    "9" => Some(Code::Digit9),
                    "F1" => Some(Code::F1),
                    "F2" => Some(Code::F2),
                    "F3" => Some(Code::F3),
                    "F4" => Some(Code::F4),
                    "F5" => Some(Code::F5),
                    "F6" => Some(Code::F6),
                    "F7" => Some(Code::F7),
                    "F8" => Some(Code::F8),
                    "F9" => Some(Code::F9),
                    "F10" => Some(Code::F10),
                    "F11" => Some(Code::F11),
                    "F12" => Some(Code::F12),
                    _ => None,
                };
            }
        }
    }
    key.map(|k| Shortcut::new(Some(mods), k))
}

fn register_hotkeys(app: &AppHandle, settings: &AppSettings) -> Vec<String> {
    let mut hotkeys = settings.hotkeys.clone();
    capto_hooks::normalize_hotkeys(&mut hotkeys);
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut conflicts = Vec::new();
    for binding in &hotkeys {
        if !binding.enabled {
            continue;
        }
        let Some(shortcut) = parse_hotkey_shortcut(&binding.shortcut) else {
            tracing::warn!(shortcut = %binding.shortcut, "skipping unparsed hotkey");
            continue;
        };
        let action = binding.action;
        let app_handle = app.clone();
        if let Err(error) = gs.on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            let app2 = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                match action {
                    HotkeyAction::StartRecording => {
                        capto_core::breadcrumbs::record("hotkey", "start_recording pressed");
                        app2.state::<AppState>()
                            .metrics
                            .incr_usage("hotkey.start_recording");
                        hotkey_start(&app2).await;
                    }
                    HotkeyAction::PauseRecording => {
                        capto_core::breadcrumbs::record("hotkey", "pause_recording pressed");
                        app2.state::<AppState>()
                            .metrics
                            .incr_usage("hotkey.pause_recording");
                        hotkey_pause(&app2).await;
                    }
                    HotkeyAction::StopRecording => {
                        capto_core::breadcrumbs::record("hotkey", "stop_recording pressed");
                        app2.state::<AppState>()
                            .metrics
                            .incr_usage("hotkey.stop_recording");
                        hotkey_stop(&app2).await;
                    }
                    HotkeyAction::TakeScreenshot => {
                        capto_core::breadcrumbs::record("hotkey", "take_screenshot pressed");
                        app2.state::<AppState>()
                            .metrics
                            .incr_usage("hotkey.take_screenshot");
                        hotkey_screenshot(&app2).await;
                    }
                }
            });
        }) {
            // Windows can briefly keep a just-unregistered shortcut alive. A
            // duplicate/conflicting binding must not make settings save or the
            // rest of the hotkeys fail; leave the existing binding in place.
            tracing::warn!(shortcut = %binding.shortcut, %error, "skipping unavailable hotkey");
            conflicts.push(binding.shortcut.clone());
            continue;
        }
        tracing::info!(shortcut = %binding.shortcut, ?action, "hotkey registered");
    }
    conflicts
}

/// FFmpeg problems are only diagnosable if its stderr actually reaches a log.
/// `CAPTO_LOG` overrides the default level (e.g. `CAPTO_LOG=capto=debug`).
fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_env("CAPTO_LOG")
        .unwrap_or_else(|_| EnvFilter::new("capto=debug,capto_core=debug,capto_encode=debug,warn"));
    let _ = fmt().with_env_filter(filter).with_target(true).try_init();
}

fn tray_labels(locale: &str) -> [&'static str; 6] {
    let lang = locale.to_ascii_lowercase();
    if lang.starts_with("zh-tw") || lang == "zh-hant" {
        [
            "顯示 Capto",
            "開始錄製",
            "暫停 / 繼續",
            "停止錄製",
            "截圖",
            "結束",
        ]
    } else if lang.starts_with("zh") {
        [
            "显示 Capto",
            "开始录制",
            "暂停 / 继续",
            "停止录制",
            "截图",
            "退出",
        ]
    } else if lang.starts_with("ja") {
        [
            "Capto を表示",
            "録画開始",
            "一時停止 / 再開",
            "録画停止",
            "スクリーンショット",
            "終了",
        ]
    } else if lang.starts_with("ko") {
        [
            "Capto 표시",
            "녹화 시작",
            "일시정지 / 재개",
            "녹화 중지",
            "스크린샷",
            "종료",
        ]
    } else if lang.starts_with("de") {
        [
            "Capto anzeigen",
            "Aufnahme starten",
            "Pause / Weiter",
            "Aufnahme stoppen",
            "Screenshot",
            "Beenden",
        ]
    } else if lang.starts_with("fr") {
        [
            "Afficher Capto",
            "Démarrer l’enregistrement",
            "Pause / Reprendre",
            "Arrêter l’enregistrement",
            "Capture d’écran",
            "Quitter",
        ]
    } else if lang.starts_with("es") {
        [
            "Mostrar Capto",
            "Iniciar grabación",
            "Pausa / Reanudar",
            "Detener grabación",
            "Captura de pantalla",
            "Salir",
        ]
    } else if lang.starts_with("pt") {
        [
            "Mostrar Capto",
            "Iniciar gravação",
            "Pausar / Retomar",
            "Parar gravação",
            "Captura de tela",
            "Sair",
        ]
    } else if lang.starts_with("ru") {
        [
            "Показать Capto",
            "Начать запись",
            "Пауза / Продолжить",
            "Остановить запись",
            "Снимок экрана",
            "Выход",
        ]
    } else {
        [
            "Show Capto",
            "Start Recording",
            "Pause / Resume",
            "Stop Recording",
            "Screenshot",
            "Quit",
        ]
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    crashlog::install_panic_hook();
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    }

    builder = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build());

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }

    builder
        .setup(|app| {
            let sidecar = sidecar_dir(app.handle());
            let settings = AppSettings::load();
            let _ = settings.ensure_output_dir();
            let session = RecordingSession::new(settings.clone(), sidecar);
            let metrics = capto_core::metrics::Metrics::new();
            metrics.incr("app_started");
            capto_core::breadcrumbs::record("lifecycle", "app_started");
            app.manage(AppState {
                session: Mutex::new(session),
                overlay: Mutex::new(None),
                audio_meter: Mutex::new(None),
                hotkey_conflicts: std::sync::Mutex::new(Vec::new()),
                metrics: metrics.clone(),
            });

            let [show_l, start_l, pause_l, stop_l, shot_l, quit_l] = tray_labels(&settings.locale);
            let show_i = MenuItem::with_id(app, "show", show_l, true, None::<&str>)?;
            let start_i = MenuItem::with_id(app, "start", start_l, true, None::<&str>)?;
            let pause_i = MenuItem::with_id(app, "pause", pause_l, true, None::<&str>)?;
            let stop_i = MenuItem::with_id(app, "stop", stop_l, true, None::<&str>)?;
            let shot_i = MenuItem::with_id(app, "shot", shot_l, true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", quit_l, true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[&show_i, &start_i, &pause_i, &stop_i, &shot_i, &quit_i],
            )?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("Capto")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        cli_server::shutdown_control_plane();
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "start" => {
                        let app2 = app.clone();
                        tauri::async_runtime::spawn(async move {
                            hotkey_start(&app2).await;
                        });
                    }
                    "pause" => {
                        let app2 = app.clone();
                        tauri::async_runtime::spawn(async move {
                            hotkey_pause(&app2).await;
                        });
                    }
                    "stop" => {
                        let app2 = app.clone();
                        tauri::async_runtime::spawn(async move {
                            hotkey_stop(&app2).await;
                        });
                    }
                    "shot" => {
                        let app2 = app.clone();
                        tauri::async_runtime::spawn(async move {
                            hotkey_screenshot(&app2).await;
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            let conflicts = register_hotkeys(app.handle(), &settings);
            if !conflicts.is_empty() {
                tracing::warn!(?conflicts, "some hotkeys could not be registered");
            }
            *app.state::<AppState>()
                .hotkey_conflicts
                .lock()
                .expect("hotkey conflict state poisoned") = conflicts;

            let register: Arc<dyn Fn(&AppHandle, &AppSettings) -> Vec<String> + Send + Sync> =
                Arc::new(|app, settings| register_hotkeys(app, settings));
            match cli_server::start_control_plane(app.handle().clone(), register, metrics.clone()) {
                Ok(port) => {
                    capto_core::breadcrumbs::record(
                        "lifecycle",
                        format!("control plane started on 127.0.0.1:{port}"),
                    );
                }
                Err(e) => {
                    tracing::error!(%e, "failed to start CLI control plane");
                    capto_core::breadcrumbs::record("lifecycle", "control plane failed to start");
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // minimize to tray instead of exit
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            get_hotkey_conflicts,
            default_output_dir,
            list_displays,
            list_windows,
            list_audio_devices,
            list_webcams,
            list_encoders,
            get_session_state,
            get_audio_levels,
            start_audio_meter,
            stop_audio_meter,
            start_recording,
            pause_recording,
            resume_recording,
            stop_recording,
            take_screenshot,
            capture_preview,
            capture_webcam_preview,
            release_preview_webcam,
            release_preview_session,
            get_overlay_defaults,
            get_platform_info,
            get_ffmpeg_info,
            open_output_folder,
            window_under_cursor,
            open_window_picker,
            close_window_picker,
            open_region_picker,
            close_region_picker,
            cursor_position,
            get_virtual_screen,
            get_window_label,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Capto")
        .run(|_app, event| {
            if let RunEvent::Exit = event {
                cli_server::shutdown_control_plane();
            }
        });
}
