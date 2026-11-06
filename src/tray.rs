//! The Mac's menu bar item: bolt, track, and a menu. On Omarchy the bar plugin does this job.
use crate::app::{Action, App};
use crate::copy;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const BOLT: [&str; 16] = [
    "................", "........XXXXXX..", ".......XXXXXX...", "......XXXXXX....", ".....XXXXXX.....", "....XXXXXXXXXX..", "...XXXXXXXXXX...", ".......XXXXX....",
    "......XXXXX.....", ".....XXXXX......", "....XXXX........", "...XXX..........", "..XX............", ".X..............", "................", "................",
];

fn icon() -> Option<Icon> {
    // Black with alpha: macOS tints a template icon to suit the menu bar.
    let rgba = BOLT.iter().flat_map(|row| row.bytes()).flat_map(|b| [0, 0, 0, if b == b'X' { 255 } else { 0 }]).collect();
    Icon::from_rgba(rgba, 16, 16).ok()
}

pub struct Tray {
    icon: TrayIcon,
    last: String,
}

impl Tray {
    /// Call once the event loop is running.
    pub fn new() -> Option<Self> {
        let icon = TrayIconBuilder::new().with_icon(icon()?).with_icon_as_template(true).build().ok()?;
        Some(Self { icon, last: String::new() })
    }

    /// Rebuilds only when what it shows changed: this runs every frame.
    pub fn update(&mut self, app: &App) {
        let track = app.now_track().map(|t| format!("{} - {}", t.artist(), t.title()));
        let top = app.top();
        let names: Vec<&str> = top.iter().map(|c| c.name.as_str()).collect();
        let sign = format!("{track:?}|{}|{names:?}", app.playing());
        if sign == self.last {
            return;
        }
        self.last = sign;
        self.icon.set_title(Some(track.as_deref().unwrap_or("").chars().take(28).collect::<String>()));
        let menu = Menu::new();
        let _ = menu.append(&MenuItem::new(track.as_deref().unwrap_or(copy::IDLE), false, None));
        let _ = menu.append(&MenuItem::with_id("toggle", if app.playing() { "Stop" } else { "Play" }, true, None));
        let _ = menu.append(&MenuItem::with_id("like", copy::LIKE, track.is_some(), None));
        let _ = menu.append(&PredefinedMenuItem::separator());
        for c in &top {
            let _ = menu.append(&MenuItem::with_id(format!("c:{}", c.id), &c.name, true, None));
        }
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::with_id("show", "show bliksem", true, None));
        let _ = menu.append(&MenuItem::with_id("quit", "fok off (quit)", true, None));
        self.icon.set_menu(Some(Box::new(menu)));
    }

    pub fn poll(app: &mut App) {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match event.id.0.as_str() {
                "toggle" => app.act(Action::Toggle),
                "like" => app.act(Action::Like),
                "show" => app.want_window = true,
                "quit" => app.want_quit = true,
                id => {
                    if let Some(c) = id.strip_prefix("c:").and_then(|n| n.parse().ok()) {
                        app.act(Action::Play(c));
                    }
                }
            }
        }
    }
}
