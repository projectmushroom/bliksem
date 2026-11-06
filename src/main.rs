//! CRT look test. No network, no audio, never touches the DI key.
//!   bliksem                                  open the window (F fullscreen, +/- zoom, Q quit)
//!   bliksem --snap out.png [--size 1224x800] [--scale 2] [--zoom 1] [--t 1.0]    render with no window
//!   bliksem --text [--size 1224x800]         print the screen as text
//!   bliksem --capture out.png                every 100 frames, save exactly what the window was handed
mod crt;
mod grid;
mod meter;
mod radio;
mod screen;

use grid::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

const FPS: u64 = 20;

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn num(name: &str, default: f32) -> f32 {
    arg(name).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn demo_art() -> image::RgbImage {
    let img = image::load_from_memory(include_bytes!("../assets/demo-art.png")).expect("bad art");
    amber_art(&img.to_rgb8())
}

/// One whole frame in font pixels. Returns the framebuffer size.
fn frame(font: &Font, art: &image::RgbImage, size: (f32, f32), zoom: f32, t: f32, out: &mut Vec<u8>) -> (u32, u32) {
    let cols = (((size.0 - 36.0) / zoom) as usize / CELL_W).max(40);
    let rows = (((size.1 - 48.0) / zoom) as usize / CELL_H).max(20);
    let mut g = Grid::new(cols, rows);
    let (x, y, w, h) = screen::draw(&mut g, t);
    paint(&g, font, out);
    let width = cols * CELL_W;
    blit_art(out, width, art, (x * CELL_W, y * CELL_H, w * CELL_W, h * CELL_H));
    (width as u32, (rows * CELL_H) as u32)
}

fn size_arg() -> (f32, f32) {
    let s = arg("--size").unwrap_or("1224x800".into());
    let (w, h) = s.split_once('x').expect("--size WxH");
    (w.parse().expect("width"), h.parse().expect("height"))
}

struct Live {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    crt: crt::Crt,
}

struct App {
    instance: wgpu::Instance,
    live: Option<Live>,
    font: Font,
    art: image::RgbImage,
    fb: Vec<u8>,
    zoom: f32,
    start: Instant,
    frames: u64,
    report: Instant,
    quit_at: Option<Instant>,
    capture: Option<String>,
    total: u64,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.live.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("BLIKSEM").with_inner_size(LogicalSize::new(1224.0, 800.0)).with_min_inner_size(LogicalSize::new(960.0, 600.0));
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs.with_titlebar_transparent(true).with_fullsize_content_view(true).with_title_hidden(true)
        };
        let window = Arc::new(el.create_window(attrs).expect("no window"));
        let surface = self.instance.create_surface(window.clone()).expect("no surface");
        let adapter = pollster::block_on(self.instance.request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() })).expect("no gpu");
        let info = adapter.get_info();
        eprintln!("gpu: {} ({:?})", info.name, info.backend);
        let caps = surface.get_capabilities(&adapter);
        // Plain 8-bit, not the first non-sRGB format: Linux lists float and 10-bit surfaces, and those change the colours.
        use wgpu::TextureFormat::{Bgra8Unorm, Bgra8UnormSrgb, Rgba8Unorm, Rgba8UnormSrgb};
        let format = [Bgra8Unorm, Rgba8Unorm, Bgra8UnormSrgb, Rgba8UnormSrgb].into_iter().find(|f| caps.formats.contains(f)).unwrap_or(caps.formats[0]);
        eprintln!("surface formats {:?}, using {format:?}", caps.formats);
        let crt = pollster::block_on(crt::Crt::new(&adapter, format));
        let px = window.inner_size();
        let mut config = surface.get_default_config(&adapter, px.width.max(1), px.height.max(1)).expect("surface not usable");
        config.format = format;
        let can_copy = caps.usages.contains(wgpu::TextureUsages::COPY_SRC);
        if self.capture.is_some() && can_copy {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        surface.configure(&crt.device, &config);
        self.live = Some(Live { window, surface, config, crt });
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(live) = self.live.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(px) if px.width > 0 && px.height > 0 => {
                live.config.width = px.width;
                live.config.height = px.height;
                live.surface.configure(&live.crt.device, &live.config);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => match event.logical_key.as_ref() {
                Key::Named(NamedKey::Escape) | Key::Character("q") => el.exit(),
                Key::Character("f") => live.window.set_fullscreen(if live.window.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) }),
                Key::Character("+") | Key::Character("=") => self.zoom = (self.zoom + 0.5).min(3.0),
                Key::Character("-") => self.zoom = (self.zoom - 0.5).max(1.0),
                _ => {}
            },
            WindowEvent::RedrawRequested => {
                let scale = live.window.scale_factor() as f32;
                let size = (live.config.width as f32 / scale, live.config.height as f32 / scale);
                let output = match live.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(o) | wgpu::CurrentSurfaceTexture::Suboptimal(o) => o,
                    wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                        live.surface.configure(&live.crt.device, &live.config);
                        return;
                    }
                    // Hidden or asleep: nothing to show, so paint nothing.
                    wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => return,
                    other => {
                        eprintln!("frame skipped: {other:?}");
                        return;
                    }
                };
                let (w, h) = frame(&self.font, &self.art, size, self.zoom, self.start.elapsed().as_secs_f32(), &mut self.fb);
                live.crt.upload(w, h, &self.fb);
                let view = output.texture.create_view(&Default::default());
                let mut encoder = live.crt.device.create_command_encoder(&Default::default());
                live.crt.draw(&view, &mut encoder, crt::View { size, scale, zoom: self.zoom });
                self.total += 1;
                match self.capture.as_ref().filter(|_| self.total % 100 == 0 && live.config.usage.contains(wgpu::TextureUsages::COPY_SRC)) {
                    Some(path) => {
                        let (pw, ph) = (live.config.width, live.config.height);
                        let mut px = live.crt.read_back(&output.texture, encoder).expect("read back");
                        if matches!(live.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
                            px.chunks_mut(4).for_each(|p| p.swap(0, 2));
                        }
                        // Via a temp name, so a reader never sees half a file.
                        let tmp = format!("{path}.tmp.png");
                        image::RgbaImage::from_raw(pw, ph, px).expect("size").save(&tmp).expect("write png");
                        std::fs::rename(&tmp, path).expect("rename");
                        eprintln!("captured {pw}x{ph} {:?} frame {}", live.config.format, self.total);
                    }
                    None => {
                        live.crt.queue.submit(Some(encoder.finish()));
                    }
                }
                live.crt.queue.present(output);
                self.frames += 1;
                if self.report.elapsed() >= Duration::from_secs(5) {
                    eprintln!("{:.1} fps, {}x{} px, zoom {}", self.frames as f32 / self.report.elapsed().as_secs_f32(), live.config.width, live.config.height, self.zoom);
                    self.frames = 0;
                    self.report = Instant::now();
                }
            }
            _ => {}
        }
    }

    // Only the timer asks for a frame. Asking on every wake spins the loop at 100% CPU, because a redraw is itself a wake.
    fn new_events(&mut self, el: &ActiveEventLoop, cause: StartCause) {
        if !matches!(cause, StartCause::ResumeTimeReached { .. } | StartCause::Init) {
            return;
        }
        if self.quit_at.is_some_and(|q| Instant::now() >= q) {
            el.exit();
        }
        if let Some(live) = &self.live {
            live.window.request_redraw();
        }
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(1000 / FPS)));
    }
}

fn main() {
    let font = Font::load();
    let art = demo_art();
    let zoom = num("--zoom", 1.0);

    if std::env::args().any(|a| a == "--text") {
        let size = size_arg();
        let mut g = Grid::new(((size.0 - 36.0) / zoom) as usize / CELL_W, ((size.1 - 48.0) / zoom) as usize / CELL_H);
        screen::draw(&mut g, num("--t", 1.0));
        println!("{}", g.text());
        return;
    }

    if let Some(path) = arg("--snap") {
        let size = size_arg();
        let scale = num("--scale", 2.0);
        let mut fb = vec![];
        let (w, h) = frame(&font, &art, size, zoom, num("--t", 1.0), &mut fb);
        let view = crt::View { size, scale, zoom };
        let (pw, ph) = ((size.0 * scale) as u32, (size.1 * scale) as u32);
        let px = pollster::block_on(crt::headless(pw, ph, (w, h, &fb), view)).expect("no gpu adapter");
        image::RgbaImage::from_raw(pw, ph, px).expect("size").save(&path).expect("write png");
        eprintln!("wrote {path} {pw}x{ph}");
        return;
    }

    // Dev hook: play one URL through the real output for N seconds, printing state and levels. Never used with a key by tests.
    if let Some(url) = arg("--play") {
        let mut r = radio::Radio::new(radio::Sink::Device);
        r.play(vec![url]);
        let secs = num("--seconds", 10.0);
        let start = Instant::now();
        while start.elapsed().as_secs_f32() < secs {
            std::thread::sleep(Duration::from_secs(1));
            let v = r.meter.read();
            println!("{:>5.1}s {:?} rendered {:.1}s rms {:.3} {:.3} peak {:.3} {:.3}", start.elapsed().as_secs_f32(), r.state(), r.rendered(), v.rms_l, v.rms_r, v.peak_l, v.peak_r);
        }
        r.stop();
        return;
    }

    if let Some(secs) = arg("--bench") {
        let size = size_arg();
        pollster::block_on(bench(&font, &art, size, num("--scale", 2.0), zoom, secs.parse().expect("seconds")));
        return;
    }

    let el = EventLoop::new().expect("no event loop");
    // Wayland needs the display connection up front to create a surface.
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(el.owned_display_handle())));
    let mut app = App {
        instance,
        live: None,
        font,
        art,
        fb: vec![],
        zoom,
        start: Instant::now(),
        frames: 0,
        report: Instant::now(),
        quit_at: arg("--quit-after").and_then(|s| s.parse().ok()).map(|s| Instant::now() + Duration::from_secs(s)),
        capture: arg("--capture"),
        total: 0,
    };
    el.run_app(&mut app).expect("event loop");
}

/// Full frames at the real frame rate with no screen: painting, upload, shader and waiting for the GPU.
async fn bench(font: &Font, art: &image::RgbImage, size: (f32, f32), scale: f32, zoom: f32, secs: f32) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("no gpu");
    eprintln!("gpu: {}", adapter.get_info().name);
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let mut crt = crt::Crt::new(&adapter, format).await;
    let (pw, ph) = ((size.0 * scale) as u32, (size.1 * scale) as u32);
    let target = crt.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: pw, height: ph, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let (start, mut fb, mut frames, mut paint_ms, mut total_ms) = (Instant::now(), vec![], 0u32, 0.0, 0.0);
    while start.elapsed().as_secs_f32() < secs {
        let t0 = Instant::now();
        let (w, h) = frame(font, art, size, zoom, start.elapsed().as_secs_f32(), &mut fb);
        paint_ms += t0.elapsed().as_secs_f32() * 1000.0;
        crt.upload(w, h, &fb);
        let mut encoder = crt.device.create_command_encoder(&Default::default());
        crt.draw(&view, &mut encoder, crt::View { size, scale, zoom });
        crt.queue.submit(Some(encoder.finish()));
        crt.device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
        total_ms += t0.elapsed().as_secs_f32() * 1000.0;
        frames += 1;
        std::thread::sleep(Duration::from_millis(1000 / FPS).saturating_sub(t0.elapsed()));
    }
    eprintln!("{pw}x{ph}: {frames} frames, paint {:.2} ms, whole frame {:.2} ms", paint_ms / frames as f32, total_ms / frames as f32);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real render path with no window. Skips where the machine has no GPU adapter at all.
    #[test]
    fn headless_render_has_black_corners_and_a_lit_screen() {
        let (font, art) = (Font::load(), demo_art());
        let mut fb = vec![];
        let size = (1224.0, 800.0);
        let (w, h) = frame(&font, &art, size, 1.0, 1.0, &mut fb);
        assert_eq!((w, h), (132 * 9, 47 * 16));
        let view = crt::View { size, scale: 1.0, zoom: 1.0 };
        let Some(px) = pollster::block_on(crt::headless(1224, 800, (w, h, &fb), view)) else {
            eprintln!("no adapter, skipped");
            return;
        };
        let at = |x: usize, y: usize| &px[(y * 1224 + x) * 4..][..3];
        assert_eq!(at(0, 0), [0, 0, 0], "corner outside the tube is black");
        let lit = px.chunks(4).filter(|p| p[0] > 120).count();
        assert!(lit > 20_000, "screen should be lit, got {lit}");
        assert!(px.chunks(4).all(|p| p[0] >= p[1] && p[1] >= p[2]), "only amber: red >= green >= blue everywhere");
    }
}
