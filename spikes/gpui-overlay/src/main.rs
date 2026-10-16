// GPUI overlay spike: proves the risky bits of a Tauri -> GPUI migration.
//   - transparent, borderless, per-display overlay windows
//   - mouse drag to draw a selection rect (RegionSelector equivalent)
//   - click-through toggle via Win32 WS_EX_TRANSPARENT (RecordOverlay equivalent)
//   - ESC quits
//
// Run: cargo run -p capto-gpui-spike

use gpui::prelude::*;
use gpui::*;

struct Overlay {
    focus: FocusHandle,
    drag_start: Option<Point<Pixels>>,
    drag_now: Option<Point<Pixels>>,
    click_through: bool,
    monitor: usize,
}

fn log_event(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(r"D:\AIWorkspace\Capto\spike-events.log")
    {
        let _ = writeln!(f, "{msg}");
    }
}

impl Overlay {
    fn new(monitor: usize, cx: &mut Context<Self>) -> Self {
        log_event(&format!("view created monitor={monitor}"));
        Self {
            focus: cx.focus_handle(),
            drag_start: None,
            drag_now: None,
            click_through: false,
            monitor,
        }
    }

    fn selection(&self) -> Option<Bounds<Pixels>> {
        let (a, b) = (self.drag_start?, self.drag_now?);
        Some(Bounds {
            origin: point(a.x.min(b.x), a.y.min(b.y)),
            size: size(
                (a.x - b.x).abs().max(px(1.0)),
                (a.y - b.y).abs().max(px(1.0)),
            ),
        })
    }
}

impl Render for Overlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = if self.click_through {
            "click-through ON (C to toggle)"
        } else {
            "interactive (C for click-through, ESC to quit)"
        };
        // paint non-primary overlays with a loud tint so we can see compositing
        let tint = if self.monitor == 0 {
            rgba(0x00000033)
        } else {
            rgba(0xaa0000aa)
        };

        let mut root = div()
            .id("overlay")
            .track_focus(&self.focus)
            .size_full()
            .bg(tint)
            .text_color(rgb(0xffffff))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match ev.keystroke.key.as_str() {
                    "escape" => cx.quit(),
                    "c" => {
                        this.click_through = !this.click_through;
                        platform::set_click_through(window, this.click_through);
                        cx.notify();
                    }
                    _ => {}
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, _w, cx| {
                    log_event(&format!(
                        "mousedown monitor={} pos=({},{})",
                        this.monitor, ev.position.x, ev.position.y
                    ));
                    this.drag_start = Some(ev.position);
                    this.drag_now = Some(ev.position);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _w, cx| {
                if this.drag_start.is_some() {
                    this.drag_now = Some(ev.position);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseUpEvent, _w, cx| {
                    this.drag_now = Some(ev.position);
                    this.drag_start = None;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .absolute()
                    .top_4()
                    .left_4()
                    .px_3()
                    .py_2()
                    .bg(rgba(0x000000aa))
                    .rounded_md()
                    .child(format!("gpui overlay spike — {state}")),
            );

        if let Some(sel) = self.selection() {
            root = root.child(
                div()
                    .absolute()
                    .left(sel.origin.x)
                    .top(sel.origin.y)
                    .w(sel.size.width)
                    .h(sel.size.height)
                    .border_2()
                    .border_color(rgb(0x33aaff))
                    .bg(rgba(0x33aaff22)),
            );
        }

        root
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        // One transparent overlay per monitor — same contract as RegionSelector.
        let displays = cx.displays();
        let _ = std::fs::remove_file(r"D:\AIWorkspace\Capto\spike-events.log");
        log_event(&format!("displays={}", displays.len()));
        for (idx, display) in displays.into_iter().enumerate() {
            let bounds = display.bounds();
            let display_id = display.id();
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: None,
                    focus: true,
                    show: true,
                    kind: WindowKind::Normal,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    display_id: Some(display_id),
                    window_background: WindowBackgroundAppearance::Transparent,
                    app_id: Some("capto-gpui-spike".into()),
                    window_min_size: None,
                    window_decorations: None,
                    tabbing_identifier: None,
                },
                |window, cx| {
                    let view = cx.new(|cx| Overlay::new(idx, cx));
                    let focus = view.read(cx).focus.clone();
                    window.focus(&focus);
                    window.activate_window();
                    platform::set_topmost(window);
                    // gpui marks transparent windows WS_EX_TRANSPARENT -> click-through by
                    // default. Strip it so the overlay can take input (region select mode).
                    platform::set_click_through(window, false);
                    view
                },
            )
            .expect("open overlay window");
        }

        cx.activate(true);
    });
}

#[cfg(windows)]
mod platform {
    use gpui::Window;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::*;

    fn hwnd(window: &Window) -> Option<HWND> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let handle = HasWindowHandle::window_handle(window).ok()?.as_raw();
        match handle {
            RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut core::ffi::c_void)),
            _ => None,
        }
    }

    /// Keep the overlay above all other windows.
    pub fn set_topmost(window: &Window) {
        if let Some(hwnd) = hwnd(window) {
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }
    }

    /// WS_EX_TRANSPARENT makes mouse events fall through to windows below.
    pub fn set_click_through(window: &Window, on: bool) {
        if let Some(hwnd) = hwnd(window) {
            unsafe {
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                let mut style = WINDOW_EX_STYLE(ex as u32);
                if on {
                    style |= WS_EX_LAYERED | WS_EX_TRANSPARENT;
                } else {
                    style &= !(WS_EX_TRANSPARENT);
                }
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style.0 as isize);
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use gpui::Window;
    pub fn set_topmost(_window: &Window) {}
    pub fn set_click_through(_window: &Window, _on: bool) {}
}
