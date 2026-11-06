//! BLIKSEM: a DI.fm player on an amber CRT, one app for Mac and Linux.
//!   bliksem                                  the player
//!   bliksem toggle|next|prev|like|status|window|show|quit      drive the running player
//!   bliksem --demo                           fake data and fake meters, never connects
//!   bliksem --snap out.png [--size 1224x800] [--scale 2] [--zoom 1] [--t 1.0]    render the demo with no window
//!   bliksem --text [--size 1224x800]         print the demo screen as text
//!   bliksem --bench 10 [--size 1224x800]     measure full frames with no screen
//!   bliksem --play URL [--seconds 10]        play one stream through the real output, printing levels
//!   bliksem --import-check                   what first launch would import from the Swift app, as counts
//!   --stream URL                             play this instead of asking DI, for testing with no key
//!   --trace                                  log clicks and shortcuts, never typed text (that could be the key)
//!   --capture out.png                        with a window: every 2 seconds, save exactly what it was handed
mod app;
mod copy;
mod crt;
mod ctl;
mod di;
mod grid;
mod media;
mod meter;
mod radio;
mod screen;
#[cfg(target_os = "macos")]
mod tray;

use app::{App, Input, Outcome};
use grid::*;
use screen::Hits;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Fullscreen, Window, WindowId};

const BOOT_HOLD: f32 = 2.6;
const BOOT_FADE: f32 = 0.9;

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn flag(name: &str) -> bool {
    std::env::args().any(|a| a == name)
}

fn num(name: &str, default: f32) -> f32 {
    arg(name).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn size_arg() -> (f32, f32) {
    let s = arg("--size").unwrap_or("1224x800".into());
    let (w, h) = s.split_once('x').expect("--size WxH");
    (w.parse().expect("width"), h.parse().expect("height"))
}

fn grid_size(size: (f32, f32), zoom: f32) -> (usize, usize) {
    ((((size.0 - 36.0) / zoom) as usize / CELL_W).max(1), (((size.1 - 48.0) / zoom) as usize / CELL_H).max(1))
}

/// Which cell a mouse position lands on: the same bend as the shader, so clicks hit what you see on the curve.
fn cell_at(pos: (f32, f32), size: (f32, f32), zoom: f32) -> Option<(usize, usize)> {
    let uv = (pos.0 / size.0 * 2.0 - 1.0, pos.1 / size.1 * 2.0 - 1.0);
    let bend = 1.0 + 0.045 * (uv.0 * uv.0 + uv.1 * uv.1);
    let flat = ((uv.0 * bend + 1.0) * 0.5 * size.0 - 18.0, (uv.1 * bend + 1.0) * 0.5 * size.1 - 30.0);
    if flat.0 < 0.0 || flat.1 < 0.0 {
        return None;
    }
    Some(((flat.0 / zoom) as usize / CELL_W, (flat.1 / zoom) as usize / CELL_H))
}

/// Paints one whole frame in font pixels. Returns the framebuffer size.
fn frame(app: &mut App, font: &Font, size: (f32, f32), boot_age: Option<f32>, hits: &mut Hits, out: &mut Vec<u8>, boot_fb: &mut Vec<u8>) -> (u32, u32) {
    let (cols, rows) = grid_size(size, app.saved.zoom);
    let mut g = Grid::new(cols, rows);
    hits.clicks.clear();
    hits.scrolls.clear();
    app.needle.step(&app.radio.meter);
    let art_at = screen::draw(&mut g, app, hits);
    paint(&g, font, out);
    let width = cols * CELL_W;
    if let Some((x, y, w, h)) = art_at {
        let rect = (x * CELL_W, y * CELL_H, w * CELL_W, h * CELL_H);
        match app.art_url().and_then(|u| app.art.get(&u).cloned()) {
            Some(art) => blit_art(out, width, &art, rect),
            None => fill(out, width, rect, ART_WAITING),
        }
    }
    if let Some(age) = boot_age.filter(|a| *a < BOOT_HOLD + BOOT_FADE) {
        let mut b = Grid::new(cols, rows);
        let logo = screen::boot(&mut b, age);
        paint(&b, font, boot_fb);
        if let Some((x, y)) = logo {
            big_text(boot_fb, width, font, x * CELL_W, y * CELL_H, copy::APP, AMBER);
        }
        // Eases out over the real screen, like the Swift boot.
        let t = ((age - BOOT_HOLD) / BOOT_FADE).clamp(0.0, 1.0);
        let show = (1.0 - t) * (1.0 - t);
        for (d, s) in out.chunks_exact_mut(4).zip(boot_fb.chunks_exact(4)) {
            for i in 0..3 {
                d[i] = (d[i] as f32 + (s[i] as f32 - d[i] as f32) * show) as u8;
            }
        }
    }
    (width as u32, (rows * CELL_H) as u32)
}

struct Live {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    crt: crt::Crt,
}

struct Player {
    instance: wgpu::Instance,
    live: Option<Live>,
    app: App,
    font: Font,
    hits: Hits,
    fb: Vec<u8>,
    boot_fb: Vec<u8>,
    demo: bool,
    start: Instant,
    frames: u64,
    captured: Instant,
    report: Instant,
    quit_at: Option<Instant>,
    capture: Option<String>,
    mouse: Option<(f32, f32)>,
    mods: ModifiersState,
    wheel: f32,
    next: Instant,
    trace: bool,
    wants_window_at_start: bool,
    focused: bool,
    #[cfg(target_os = "macos")]
    tray: Option<tray::Tray>,
}

impl Player {
    fn size_points(live: &Live) -> (f32, f32) {
        let scale = live.window.scale_factor() as f32;
        (live.config.width as f32 / scale, live.config.height as f32 / scale)
    }

    fn cell(&self) -> Option<(usize, usize)> {
        let live = self.live.as_ref()?;
        cell_at(self.mouse?, Self::size_points(live), self.app.saved.zoom)
    }

    fn redraw_soon(&self) {
        if let Some(live) = &self.live {
            live.window.request_redraw();
        }
    }

    fn outcome(&mut self, el: &ActiveEventLoop, outcome: Outcome) {
        match outcome {
            Outcome::Nothing => {}
            Outcome::Quit => el.exit(),
            Outcome::Fullscreen => {
                if let Some(live) = &self.live {
                    live.window.set_fullscreen(if live.window.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) });
                }
            }
            Outcome::Paste => {
                let (tool, args): (&str, &[&str]) = if cfg!(target_os = "macos") { ("pbpaste", &[]) } else { ("wl-paste", &["--no-newline"]) };
                if let Ok(o) = std::process::Command::new(tool).args(args).output() {
                    self.app.paste(&String::from_utf8_lossy(&o.stdout));
                }
            }
        }
    }

    fn open_window(&mut self, el: &ActiveEventLoop) {
        let attrs = Window::default_attributes().with_title(copy::APP).with_inner_size(LogicalSize::new(1224.0, 800.0)).with_min_inner_size(LogicalSize::new(960.0, 600.0));
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs.with_titlebar_transparent(true).with_fullsize_content_view(true).with_title_hidden(true)
        };
        #[cfg(target_os = "linux")]
        let attrs = {
            use winit::platform::wayland::WindowAttributesExtWayland;
            attrs.with_name("bliksem", "bliksem")
        };
        let window = Arc::new(el.create_window(attrs).expect("no window"));
        let surface = self.instance.create_surface(window.clone()).expect("no surface");
        let adapter = pollster::block_on(self.instance.request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() })).expect("no gpu");
        let caps = surface.get_capabilities(&adapter);
        // Plain 8-bit, not the first non-sRGB format: Linux lists float and 10-bit surfaces, and those change the colours.
        use wgpu::TextureFormat::{Bgra8Unorm, Bgra8UnormSrgb, Rgba8Unorm, Rgba8UnormSrgb};
        let format = [Bgra8Unorm, Rgba8Unorm, Bgra8UnormSrgb, Rgba8UnormSrgb].into_iter().find(|f| caps.formats.contains(f)).unwrap_or(caps.formats[0]);
        let crt = pollster::block_on(crt::Crt::new(&adapter, format));
        let px = window.inner_size();
        let mut config = surface.get_default_config(&adapter, px.width.max(1), px.height.max(1)).expect("surface not usable");
        config.format = format;
        // A hidden Wayland window gets no frame callbacks, and a blocking present would freeze the player's clock.
        config.present_mode = wgpu::PresentMode::AutoNoVsync;
        if self.capture.is_some() && caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        surface.configure(&crt.device, &config);
        self.live = Some(Live { window, surface, config, crt });
    }

    fn render(&mut self) {
        let Some(live) = self.live.as_mut() else { return };
        let scale = live.window.scale_factor() as f32;
        let size = Self::size_points(live);
        // Asking first: a hidden window has nothing to show, so it paints nothing.
        let output = match live.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(o) | wgpu::CurrentSurfaceTexture::Suboptimal(o) => o,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                live.surface.configure(&live.crt.device, &live.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => return,
            other => {
                eprintln!("frame skipped: {other:?}");
                return;
            }
        };
        let age = self.start.elapsed().as_secs_f32();
        let (w, h) = frame(&mut self.app, &self.font, size, Some(age), &mut self.hits, &mut self.fb, &mut self.boot_fb);
        live.crt.upload(w, h, &self.fb);
        let view = output.texture.create_view(&Default::default());
        let mut encoder = live.crt.device.create_command_encoder(&Default::default());
        live.crt.draw(&view, &mut encoder, crt::View { size, scale, zoom: self.app.saved.zoom });
        let due = self.captured.elapsed() >= Duration::from_secs(2) && live.config.usage.contains(wgpu::TextureUsages::COPY_SRC);
        match self.capture.as_ref().filter(|_| due) {
            Some(path) => {
                self.captured = Instant::now();
                let (pw, ph) = (live.config.width, live.config.height);
                let mut px = live.crt.read_back(&output.texture, encoder).expect("read back");
                if matches!(live.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
                    px.chunks_mut(4).for_each(|p| p.swap(0, 2));
                }
                // Via a temp name, so a reader never sees half a file.
                let tmp = format!("{path}.tmp.png");
                image::RgbaImage::from_raw(pw, ph, px).expect("size").save(&tmp).expect("write png");
                std::fs::rename(&tmp, path).expect("rename");
            }
            None => {
                live.crt.queue.submit(Some(encoder.finish()));
            }
        }
        live.crt.queue.present(output);
        self.frames += 1;
        if self.report.elapsed() >= Duration::from_secs(5) && self.capture.is_some() {
            eprintln!("{:.1} fps, {}x{} px, zoom {}", self.frames as f32 / self.report.elapsed().as_secs_f32(), live.config.width, live.config.height, self.app.saved.zoom);
            self.frames = 0;
            self.report = Instant::now();
        }
    }

    /// Smooth while anything moves, near idle when nothing does.
    fn frame_gap(&self) -> Duration {
        let moving = self.start.elapsed().as_secs_f32() < BOOT_HOLD + BOOT_FADE || self.app.playing() || self.app.status == copy::LOADING || self.app.needle.l > 0.001 || self.app.needle.peak_l > 0.001;
        Duration::from_millis(if moving { 50 } else { 250 })
    }
}

impl ApplicationHandler for Player {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        #[cfg(target_os = "macos")]
        if self.tray.is_none() && !self.demo {
            self.tray = tray::Tray::new();
        }
        if self.live.is_none() && self.wants_window_at_start {
            self.wants_window_at_start = false;
            self.open_window(el);
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.live.is_none() {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                // The music outlives the window. Nothing playing means nothing to stay for, except the Mac's menu bar item.
                if self.app.wants_music() || cfg!(target_os = "macos") {
                    self.live = None;
                    self.focused = false;
                } else {
                    el.exit();
                }
            }
            WindowEvent::Focused(f) => self.focused = f,
            WindowEvent::Resized(px) if px.width > 0 && px.height > 0 => {
                let live = self.live.as_mut().unwrap();
                live.config.width = px.width;
                live.config.height = px.height;
                live.surface.configure(&live.crt.device, &live.config);
                self.redraw_soon();
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let command = if cfg!(target_os = "macos") { self.mods.super_key() } else { self.mods.control_key() };
                let input = match event.logical_key.as_ref() {
                    Key::Character(c) if command => c.chars().next().map(Input::Command),
                    Key::Named(NamedKey::Backspace) => Some(Input::Backspace),
                    Key::Named(NamedKey::Enter) => Some(Input::Enter),
                    Key::Named(NamedKey::Escape) => {
                        // Out of fullscreen before anything else, when nothing is being typed.
                        let live = self.live.as_ref().unwrap();
                        if self.app.focus == app::Focus::None && live.window.fullscreen().is_some() {
                            live.window.set_fullscreen(None);
                            None
                        } else {
                            Some(Input::Escape)
                        }
                    }
                    Key::Named(NamedKey::ArrowUp) => Some(Input::Up),
                    Key::Named(NamedKey::ArrowDown) => Some(Input::Down),
                    Key::Named(NamedKey::F11) => {
                        self.outcome(el, Outcome::Fullscreen);
                        None
                    }
                    _ if !command => event.text.map(|t| Input::Text(t.to_string())),
                    _ => None,
                };
                if let Some(input) = input {
                    if self.trace && !matches!(input, Input::Text(_)) {
                        eprintln!("key {input:?} focus {:?}", self.app.focus);
                    }
                    let outcome = self.app.input(input);
                    self.outcome(el, outcome);
                }
                self.redraw_soon();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let live = self.live.as_ref().unwrap();
                let p: PhysicalPosition<f64> = position;
                let scale = live.window.scale_factor();
                self.mouse = Some(((p.x / scale) as f32, (p.y / scale) as f32));
                let over = self.cell().is_some_and(|(x, y)| self.hits.action_at(x, y).is_some());
                self.live.as_ref().unwrap().window.set_cursor(if over { CursorIcon::Pointer } else { CursorIcon::Default });
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                if self.trace {
                    eprintln!("click at {:?} cell {:?} action {:?}", self.mouse, self.cell(), self.cell().and_then(|(x, y)| self.hits.action_at(x, y)));
                }
                if let Some(action) = self.cell().and_then(|(x, y)| self.hits.action_at(x, y).cloned()) {
                    self.app.act(action);
                } else {
                    self.app.focus = app::Focus::None;
                }
                self.redraw_soon();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -y,
                    MouseScrollDelta::PixelDelta(p) => -(p.y as f32) / (CELL_H as f32 * self.app.saved.zoom),
                };
                self.wheel += lines;
                let whole = self.wheel.trunc();
                if whole != 0.0 {
                    self.wheel -= whole;
                    if let Some(s) = self.cell().and_then(|(x, y)| self.hits.scroll_at(x, y)) {
                        self.app.scroll_by(s, whole as i32);
                    }
                }
                self.redraw_soon();
            }
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }

    fn new_events(&mut self, el: &ActiveEventLoop, cause: StartCause) {
        // Only the timer starts a frame: a redraw is itself a wake, so asking on every wake spins at 100% CPU.
        if !matches!(cause, StartCause::ResumeTimeReached { .. } | StartCause::Init | StartCause::WaitCancelled { .. }) {
            return;
        }
        let now = Instant::now();
        if now >= self.next {
            if self.app.want_quit || self.quit_at.is_some_and(|q| now >= q) {
                el.exit();
            }
            if std::mem::take(&mut self.app.flip_window) {
                // The bar's click: open it, bring it up, or put it away.
                if self.live.is_some() && self.focused {
                    self.live = None;
                    self.focused = false;
                } else {
                    self.app.want_window = true;
                }
            }
            if std::mem::take(&mut self.app.want_window) {
                match &self.live {
                    Some(live) => live.window.focus_window(),
                    None => self.open_window(el),
                }
            }
            // The clock and the radio state run here, not in the paint, so they keep going while the window is hidden.
            self.app.update();
            #[cfg(target_os = "macos")]
            {
                tray::Tray::poll(&mut self.app);
                if let Some(t) = &mut self.tray {
                    t.update(&self.app);
                }
            }
            if self.demo {
                self.app.radio.meter.push_demo(self.start.elapsed().as_secs_f32());
            }
            self.redraw_soon();
            self.next = now + self.frame_gap();
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next));
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        self.app.save();
    }
}

/// Demo meters as if they had been running for a while.
fn warm_up(app: &mut App, t: f32) {
    for i in 0..512 {
        app.radio.meter.push_demo(t - (512 - i) as f32 * 0.05);
    }
    for _ in 0..12 {
        app.needle.step(&app.radio.meter);
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn main() {
    // A word on its own drives the running player. `show` starts one if there isn't one.
    let word = std::env::args().nth(1).filter(|w| ctl::COMMANDS.contains(&w.as_str()));
    if let Some(reply) = word.as_deref().and_then(ctl::send) {
        print!("{reply}");
        return;
    }
    if word.as_deref().is_some_and(|w| w != "show" && w != "window") {
        eprintln!("bliksem is not running");
        std::process::exit(1);
    }
    let dev = ["--text", "--snap", "--play", "--bench", "--demo", "--import-check"].iter().any(|f| flag(f));
    if !dev && word.is_none() && ctl::send("show").is_some() {
        return; // already running: this just brought its window up
    }

    if flag("--import-check") {
        println!("{}", app::import_check());
        return;
    }

    if flag("--text") {
        let size = size_arg();
        let (cols, rows) = grid_size(size, num("--zoom", 1.0));
        let mut app = App::demo();
        warm_up(&mut app, num("--t", 1.0));
        let mut g = Grid::new(cols, rows);
        screen::draw(&mut g, &mut app, &mut Hits::default());
        println!("{}", g.text());
        return;
    }

    if let Some(path) = arg("--snap") {
        let size = size_arg();
        let scale = num("--scale", 2.0);
        let mut app = App::demo();
        app.saved.zoom = num("--zoom", 1.0);
        warm_up(&mut app, num("--t", 1.0));
        let (font, mut fb, mut boot_fb) = (Font::load(), vec![], vec![]);
        let boot_age = arg("--boot").and_then(|v| v.parse().ok());
        let (w, h) = frame(&mut app, &font, size, boot_age, &mut Hits::default(), &mut fb, &mut boot_fb);
        let view = crt::View { size, scale, zoom: app.saved.zoom };
        let (pw, ph) = ((size.0 * scale) as u32, (size.1 * scale) as u32);
        let px = pollster::block_on(crt::headless(pw, ph, (w, h, &fb), view)).expect("no gpu adapter");
        image::RgbaImage::from_raw(pw, ph, px).expect("size").save(&path).expect("write png");
        eprintln!("wrote {path} {pw}x{ph}");
        return;
    }

    // Dev hook: play one URL through the real output, printing state and levels.
    if let Some(url) = arg("--play") {
        let mut r = radio::Radio::new(radio::Sink::Device);
        r.play(vec![url]);
        let start = Instant::now();
        while start.elapsed().as_secs_f32() < num("--seconds", 10.0) {
            std::thread::sleep(Duration::from_secs(1));
            let v = r.meter.read();
            println!("{:>5.1}s {:?} rendered {:.1}s rms {:.3} {:.3} peak {:.3} {:.3}", start.elapsed().as_secs_f32(), r.state(), r.rendered(), v.rms_l, v.rms_r, v.peak_l, v.peak_r);
        }
        r.stop();
        return;
    }

    if let Some(secs) = arg("--bench") {
        pollster::block_on(bench(size_arg(), num("--scale", 2.0), num("--zoom", 1.0), secs.parse().expect("seconds")));
        return;
    }

    let demo = flag("--demo");
    let el = EventLoop::new().expect("no event loop");
    // Wayland needs the display connection up front to create a surface.
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(el.owned_display_handle())));
    let mut opened = if demo { App::demo() } else { App::open() };
    opened.stream_override = arg("--stream");
    let mut player = Player {
        instance,
        live: None,
        app: opened,
        font: Font::load(),
        hits: Hits::default(),
        fb: vec![],
        boot_fb: vec![],
        demo,
        start: Instant::now(),
        frames: 0,
        captured: Instant::now(),
        report: Instant::now(),
        quit_at: arg("--quit-after").and_then(|s| s.parse().ok()).map(|s| Instant::now() + Duration::from_secs(s)),
        capture: arg("--capture"),
        mouse: None,
        mods: ModifiersState::empty(),
        wheel: 0.0,
        next: Instant::now(),
        trace: flag("--trace"),
        wants_window_at_start: true,
        focused: false,
        #[cfg(target_os = "macos")]
        tray: None,
    };
    el.run_app(&mut player).expect("event loop");
}

/// Full frames at the real frame rate with no screen: painting, upload, shader and waiting for the GPU.
async fn bench(size: (f32, f32), scale: f32, zoom: f32, secs: f32) {
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
    let (mut app, font, mut hits, mut boot_fb) = (App::demo(), Font::load(), Hits::default(), vec![]);
    app.saved.zoom = zoom;
    let (start, mut fb, mut frames, mut paint_ms, mut total_ms) = (Instant::now(), vec![], 0u32, 0.0, 0.0);
    while start.elapsed().as_secs_f32() < secs {
        let t0 = Instant::now();
        app.radio.meter.push_demo(start.elapsed().as_secs_f32());
        let (w, h) = frame(&mut app, &font, size, None, &mut hits, &mut fb, &mut boot_fb);
        paint_ms += t0.elapsed().as_secs_f32() * 1000.0;
        crt.upload(w, h, &fb);
        let mut encoder = crt.device.create_command_encoder(&Default::default());
        crt.draw(&view, &mut encoder, crt::View { size, scale, zoom });
        crt.queue.submit(Some(encoder.finish()));
        crt.device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
        total_ms += t0.elapsed().as_secs_f32() * 1000.0;
        frames += 1;
        std::thread::sleep(Duration::from_millis(50).saturating_sub(t0.elapsed()));
    }
    eprintln!("{pw}x{ph}: {frames} frames, paint {:.2} ms, whole frame {:.2} ms", paint_ms / frames as f32, total_ms / frames as f32);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real render path with no window. Skips where the machine has no GPU adapter at all.
    #[test]
    fn headless_render_has_black_corners_and_only_amber() {
        let (mut app, font, mut hits, mut fb, mut boot_fb) = (App::demo(), Font::load(), Hits::default(), vec![], vec![]);
        let size = (1224.0, 800.0);
        let (w, h) = frame(&mut app, &font, size, None, &mut hits, &mut fb, &mut boot_fb);
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

    #[test]
    fn a_click_finds_its_cell_through_the_curve() {
        let size = (1224.0, 800.0);
        // The middle of the screen barely bends.
        let (x, y) = cell_at((612.0, 400.0), size, 1.0).unwrap();
        assert_eq!((x, y), ((612 - 18) / 9, (400 - 30) / 16));
        // Near the top left the tube bends away, so the same spot is further out than a flat screen would say.
        let flat_x = (60.0f32 - 18.0) as usize / 9;
        let (bent_x, _) = cell_at((60.0, 100.0), size, 1.0).unwrap();
        assert!(bent_x < flat_x, "{bent_x} vs {flat_x}");
        assert_eq!(cell_at((0.0, 0.0), size, 1.0), None, "outside the tube is nothing");
        // Zoom makes cells bigger.
        assert_eq!(cell_at((612.0, 400.0), size, 2.0).unwrap(), ((612 - 18) / 2 / 9, (400 - 30) / 2 / 16));
    }

    /// The whole loop with no window: a click at a real pixel position, through the curve, reaches the app.
    #[test]
    fn a_pixel_click_on_a_favourite_star_changes_the_app() {
        let (mut app, font, mut hits, mut fb, mut boot_fb) = (App::demo(), Font::load(), Hits::default(), vec![], vec![]);
        let size = (1224.0, 800.0);
        frame(&mut app, &font, size, None, &mut hits, &mut fb, &mut boot_fb);
        let (r, _) = hits.clicks.iter().rev().find(|(_, a)| *a == app::Action::ToggleFav(37)).unwrap();
        // Walk the window for a pixel that lands on the star: that is what a user's mouse does.
        let pos = (0..1224).step_by(3).flat_map(|x| (0..800).step_by(3).map(move |y| (x as f32, y as f32))).find(|p| cell_at(*p, size, 1.0) == Some((r.x, r.y))).expect("the star is on screen");
        let (x, y) = cell_at(pos, size, 1.0).unwrap();
        let action = hits.action_at(x, y).unwrap().clone();
        assert!(app.is_fav(37));
        app.act(action);
        assert!(!app.is_fav(37));
    }
}
