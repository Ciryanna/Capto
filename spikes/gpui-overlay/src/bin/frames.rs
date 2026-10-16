// GPUI frame-texture spike: proves a capture-style frame buffer can be pushed
// into a GPUI texture at ~30fps without IPC/base64 (the preview path a
// Tauri->GPUI migration would use instead of `capture_preview` polling).
//
// A background task produces BGRA frames (moving gradient + white bar) ->
// RenderImage -> img element. Old atlas tiles are evicted via drop_image.
// HUD shows measured fps + frame-gen cost. ESC quits.
//
// Run: cargo run -p capto-gpui-spike --bin gpui-frames-spike

use gpui::prelude::*;
use gpui::*;
use smallvec::smallvec;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

const W: u32 = 1280;
const H: u32 = 720;

fn gradient_base() -> Vec<u8> {
    let mut buf = vec![0u8; (W * H * 4) as usize];
    for y in 0..H {
        for x in 0..W {
            let i = ((y * W + x) * 4) as usize;
            buf[i] = (x as f32 / W as f32 * 255.0) as u8; // B: horizontal gradient
            buf[i + 1] = (y as f32 / H as f32 * 255.0) as u8; // G: vertical gradient
            buf[i + 2] = 40;
            buf[i + 3] = 255;
        }
    }
    buf
}

fn gen_frame(base: &[u8], tick: usize) -> image::ImageBuffer<image::Rgba<u8>, Vec<u8>> {
    let mut buf = base.to_vec(); // one memcpy, ~3.7MB
                                 // moving white bar makes frame-to-frame motion obvious
    let bar_x = (tick as i64 * 12) % (W as i64 + 160) - 80;
    for x in bar_x.max(0)..(bar_x + 80).min(W as i64) {
        for y in 0..H {
            let i = ((y * W + x as u32) * 4) as usize;
            buf[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
    }
    image::ImageBuffer::from_raw(W, H, buf).unwrap()
}

struct Video {
    focus: FocusHandle,
    frame: Arc<RenderImage>,
    stale: Option<Arc<RenderImage>>,
    gen_ms: f32,
    count: usize,
    recent: VecDeque<Instant>,
}

impl Video {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |view, cx| {
            let base = gradient_base();
            let mut tick = 0usize;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                tick += 1;
                let t0 = Instant::now();
                let image = RenderImage::new(smallvec![image::Frame::new(gen_frame(&base, tick))]);
                let gen = t0.elapsed();
                let ok = view
                    .update(cx, |this, cx| {
                        let old = std::mem::replace(&mut this.frame, Arc::new(image));
                        this.stale = Some(old);
                        this.gen_ms = gen.as_secs_f32() * 1000.0;
                        this.count = tick;
                        this.recent.push_back(Instant::now());
                        while this
                            .recent
                            .front()
                            .is_some_and(|t| t.elapsed() > Duration::from_secs(1))
                        {
                            this.recent.pop_front();
                        }
                        cx.notify();
                    })
                    .is_ok();
                if !ok {
                    break;
                }
            }
        })
        .detach();

        Self {
            focus: cx.focus_handle(),
            frame: Arc::new(RenderImage::new(smallvec![image::Frame::new(
                image::ImageBuffer::from_pixel(W, H, image::Rgba([0, 0, 0, 255])),
            )])),
            stale: None,
            gen_ms: 0.0,
            count: 0,
            recent: VecDeque::new(),
        }
    }
}

impl Render for Video {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(stale) = self.stale.take() {
            cx.drop_image(stale, Some(window));
        }

        div()
            .track_focus(&self.focus)
            .size_full()
            .bg(rgb(0x000000))
            .on_key_down(cx.listener(|_this, ev: &KeyDownEvent, _w, cx| {
                if ev.keystroke.key.as_str() == "escape" {
                    cx.quit();
                }
            }))
            .child(
                img(self.frame.clone())
                    .size_full()
                    .object_fit(ObjectFit::Cover),
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
                    .text_color(rgb(0xffffff))
                    .child(format!(
                        "gpui frames spike — frame {} | gen {:.1}ms | fps {}",
                        self.count,
                        self.gen_ms,
                        self.recent.len()
                    )),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds {
            origin: point(px(200.0), px(120.0)),
            size: size(px(960.0), px(560.0)),
        };
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                focus: true,
                show: true,
                kind: WindowKind::Normal,
                is_movable: true,
                is_resizable: false,
                is_minimizable: false,
                display_id: None,
                window_background: WindowBackgroundAppearance::Opaque,
                app_id: Some("capto-gpui-frames".into()),
                window_min_size: None,
                window_decorations: None,
                tabbing_identifier: None,
            },
            |window, cx| {
                let view = cx.new(Video::new);
                let focus = view.read(cx).focus.clone();
                window.focus(&focus);
                window.activate_window();
                view
            },
        )
        .expect("open window");
        cx.activate(true);
    });
}
