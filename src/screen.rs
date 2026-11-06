//! Draws the app onto the grid, cell for cell like the Swift `Screen`, and records what each cell does when clicked.
use crate::app::{Action, App, FAV_SECTION, Focus, MODULES, Module, Scroll, fit_modules, hm};
use crate::copy;
use crate::di;
use crate::grid::*;
use crate::meter::wave_level;

const NOW_ROWS: usize = 12;
const ART_COLS: usize = 21;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

impl Rect {
    fn contains(&self, x: usize, y: usize) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// What the mouse can do this frame. Later entries sit on top of earlier ones.
#[derive(Default)]
pub struct Hits {
    pub clicks: Vec<(Rect, Action)>,
    pub scrolls: Vec<(Rect, Scroll)>,
}

impl Hits {
    pub fn action_at(&self, x: usize, y: usize) -> Option<&Action> {
        self.clicks.iter().rev().find(|(r, _)| r.contains(x, y)).map(|(_, a)| a)
    }
    pub fn scroll_at(&self, x: usize, y: usize) -> Option<Scroll> {
        self.scrolls.iter().rev().find(|(r, _)| r.contains(x, y)).map(|(_, s)| *s)
    }
    fn click(&mut self, x: usize, y: usize, w: usize, action: Action) {
        self.clicks.push((Rect { x, y, w, h: 1 }, action));
    }
}

fn len(s: &str) -> usize {
    s.chars().count()
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn boxed(g: &mut Grid, x: usize, y: usize, w: usize, h: usize, title: Option<&str>) {
    if w < 2 || h < 2 {
        return;
    }
    let inner = w - 2;
    if let Some(title) = title {
        let line = format!("╔═[ {title} ]{}", "═".repeat(inner.saturating_sub(len(title) + 5)));
        g.put(x, y, &clip(&line, inner + 1), AMBER);
        g.put(x + w - 1, y, "╗", AMBER);
    }
    for row in y + 1..y + h - 1 {
        g.put(x, row, "║", AMBER);
        g.put(x + w - 1, row, "║", AMBER);
    }
    g.put(x, y + h - 1, &format!("╚{}╝", "═".repeat(inner)), AMBER);
}

fn button(g: &mut Grid, hits: &mut Hits, x: usize, y: usize, label: &str, fg: Rgb, bg: Option<Rgb>, action: Action) {
    g.put_on(x, y, label, fg, bg);
    hits.click(x, y, len(label), action);
}

/// One text box on the grid. The row focuses it; a button at the end clears or submits.
#[allow(clippy::too_many_arguments)]
fn text_box(g: &mut Grid, hits: &mut Hits, x: usize, y: usize, w: usize, prompt: &str, text: &str, secret: bool, placeholder: &str, focused: bool, blink: bool, field: Focus, end: Option<(&str, Action)>) {
    hits.click(x, y, w, Action::Focus(field));
    if focused {
        g.put_on(x, y, prompt, BG, Some(AMBER));
    } else {
        g.put(x, y, prompt, AMBER);
    }
    let at = x + len(prompt);
    let room = (x + w).saturating_sub(at);
    let shown = if secret { "*".repeat(len(text).min(8)) } else { text.to_string() };
    let end_w = if text.is_empty() { 0 } else { end.as_ref().map_or(0, |(l, _)| len(l)) };
    let room_text = room.saturating_sub(end_w + 1);
    if shown.is_empty() && !focused {
        g.put(at, y, &clip(placeholder, room), DIM);
    } else {
        // Keep the end of a long entry in view, where the cursor is.
        let skip = len(&shown).saturating_sub(room_text);
        let tail: String = shown.chars().skip(skip).collect();
        g.put(at, y, &tail, BRIGHT);
        if focused && blink {
            g.put(at + len(&tail), y, "█", AMBER);
        }
    }
    if !text.is_empty() {
        if let Some((label, action)) = end {
            button(g, hits, x + w - len(label), y, label, AMBER, None, action);
        }
    }
}

fn sidebar(g: &mut Grid, app: &mut App, hits: &mut Hits, cols: usize, rows: usize) {
    if cols < 3 || rows < 8 {
        return;
    }
    let inner = cols - 2;
    let blink = (app.started.elapsed().as_secs_f32() * 2.0) as u64 % 2 == 0;
    let has_key = !app.key().is_empty();
    text_box(g, hits, 1, 1, inner, copy::FIND, &app.query, false, copy::SEARCH, app.focus == Focus::Find, blink, Focus::Find, Some(("[x]", Action::ClearFind)));
    g.put(1, 2, &"─".repeat(inner), AMBER);

    let hint = usize::from(!has_key);
    let list_y = 3;
    let list_h = rows.saturating_sub(list_y + 3 + hint);
    let current = app.current.as_ref().map(|c| c.id);

    // Flatten to lines first: a channel takes two when it shows what's on.
    enum Line {
        Head(&'static str, String),
        Channel(di::Channel),
        Sub(di::Channel),
        Empty,
    }
    let mut lines = vec![];
    for s in app.sections() {
        let closed = app.saved.collapsed.contains(s.id);
        lines.push(Line::Head(s.id, format!(" {} {} ({})", if closed { "►" } else { "▼" }, s.title, s.channels.len())));
        if closed {
            continue;
        }
        if s.id == FAV_SECTION && s.channels.is_empty() {
            lines.push(Line::Empty);
        }
        for c in s.channels {
            let on = current == Some(c.id);
            let live = (s.id == FAV_SECTION || on) && app.on_air.contains_key(&c.id);
            lines.push(Line::Channel(c.clone()));
            if live {
                lines.push(Line::Sub(c));
            }
        }
    }

    if app.reveal {
        app.reveal = false;
        if let Some(at) = lines.iter().position(|l| matches!(l, Line::Channel(c) if Some(c.id) == current)) {
            let top = app.scroll.get(&Scroll::Sidebar).copied().unwrap_or(0);
            // Bring the section heading along when this is the first channel under it.
            let first = if at > 0 && matches!(lines[at - 1], Line::Head(..)) { at - 1 } else { at };
            let new = if first < top { first } else if at + 2 > top + list_h { (at + 2).saturating_sub(list_h) } else { top };
            app.scroll.insert(Scroll::Sidebar, new);
        }
    }
    let top = scrolled(app, Scroll::Sidebar, lines.len(), list_h);
    hits.scrolls.push((Rect { x: 1, y: list_y, w: inner, h: list_h }, Scroll::Sidebar));

    for (i, line) in lines.iter().skip(top).take(list_h).enumerate() {
        let y = list_y + i;
        match line {
            Line::Head(id, text) => {
                g.put(1, y, &clip(text, inner), BRIGHT);
                hits.click(1, y, inner, Action::ToggleSection(id));
            }
            Line::Empty => g.put(1, y, &clip(copy::NO_FAVS, inner), DIM),
            Line::Channel(c) => {
                let on = current == Some(c.id);
                let fav = app.is_fav(c.id);
                let bg = on.then_some(AMBER);
                let ink = if on { BG } else { AMBER };
                g.put_on(1, y, &" ".repeat(inner), ink, bg);
                let time = hm(app.saved.seconds.get(&c.key).copied().unwrap_or(0));
                let room = inner.saturating_sub(3 + len(&time) + 1);
                g.put_on(4, y, &clip(&format!(" {}", c.name.to_uppercase()), room), ink, bg);
                g.put_on(1 + inner - len(&time) - 1, y, &time, ink, bg);
                hits.click(1, y, inner, Action::Play(c.id));
                button(g, hits, 1, y, if fav { "[*]" } else { "[ ]" }, if on { BG } else if fav { BRIGHT } else { DIM }, bg, Action::ToggleFav(c.id));
            }
            Line::Sub(c) => {
                let on = current == Some(c.id);
                let bg = on.then_some(AMBER);
                g.put_on(1, y, &" ".repeat(inner), BG, bg);
                if let Some(t) = app.on_air.get(&c.id) {
                    let text = format!("    {} - {}", t.artist(), t.title());
                    g.put_on(1, y, &clip(&text, inner), if on { BG } else { DIM }, bg);
                }
                hits.click(1, y, inner, Action::Play(c.id));
            }
        }
    }

    let foot = rows - 3 - hint;
    g.put(1, foot, &"─".repeat(inner), AMBER);
    let key = app.saved.key.clone();
    text_box(g, hits, 1, foot + 1, inner, copy::KEY_PROMPT, &key, true, copy::KEY_PLACEHOLDER, app.focus == Focus::Key, blink, Focus::Key, Some((copy::OK, Action::SubmitKey)));
    if !has_key {
        g.put(1, foot + 2, &clip(copy::KEY_HINT, inner), DIM);
    }
}

/// The scroll position for a list, held inside what the list can actually show.
fn scrolled(app: &mut App, s: Scroll, lines: usize, rows: usize) -> usize {
    let v = app.scroll.get(&s).copied().unwrap_or(0).min(lines.saturating_sub(rows));
    app.scroll.insert(s, v);
    v
}

/// Word wrap, at most `max` lines.
fn wrap(s: &str, width: usize, max: usize) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut line = String::new();
    for word in s.split_whitespace() {
        if !line.is_empty() && len(&line) + 1 + len(word) > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out.truncate(max);
    out.into_iter().map(|l| clip(&l, width)).collect()
}

/// Returns the art's place in cells when it fits.
fn now_pane(g: &mut Grid, app: &App, hits: &mut Hits, x: usize, y: usize, cols: usize) -> Option<(usize, usize, usize, usize)> {
    let Some(c) = &app.current else {
        g.put(x, y, &clip(&format!(" {}", copy::IDLE), cols), BRIGHT);
        return None;
    };
    let show_art = cols >= ART_COLS + 20;
    let tx = if show_art { x + ART_COLS + 1 } else { x };
    let w = (x + cols).saturating_sub(tx);
    g.put(tx, y, &clip(&format!(" DI.FM {}", c.name.to_uppercase()), w), BRIGHT);
    let track = app.now_track();
    let title = track.map_or(copy::NOTHING, |t| t.title());
    for (i, line) in wrap(title, w.saturating_sub(1), 3).iter().enumerate() {
        g.put(tx + 1, y + 2 + i, line, BRIGHT);
    }
    g.put(tx, y + 5, &clip(&format!(" {}", track.map_or("", |t| t.artist())), w), AMBER);
    let votes = track.and_then(|t| app.history.iter().find(|h| h.track_id == t.id)).and_then(|h| h.votes).map_or(String::new(), |v| format!("▲{} ▼{}   ", v.up, v.down));
    let time = hm(app.saved.seconds.get(&c.key).copied().unwrap_or(0));
    let listened = if time.is_empty() { String::new() } else { format!("{time} {}", copy::ON_CHANNEL) };
    g.put(tx, y + 7, &clip(&format!(" {votes}{listened}"), w), DIM);
    let label = if app.playing() { copy::STOP } else { copy::PLAY };
    button(g, hits, tx + 1, y + 9, label, AMBER, None, Action::Toggle);
    if let Some(t) = track {
        let liked = app.is_liked(t.id);
        let (text, fg, bg) = if liked { (copy::LIKED, BG, Some(AMBER)) } else { (copy::LIKE, AMBER, None) };
        button(g, hits, tx + 1 + len(label) + 2, y + 9, text, fg, bg, Action::Like);
    }
    if app.status == copy::LOADING {
        let n = (app.started.elapsed().as_secs_f32() * 10.0) as usize % 21;
        g.put(tx, y + 11, &clip(&format!(" [{}{}] {}", "█".repeat(n), "░".repeat(20 - n), copy::LOADING), w), AMBER);
    } else {
        g.put(tx, y + 11, &clip(&format!(" {}", app.status), w), BRIGHT);
    }
    show_art.then_some((x, y, ART_COLS, NOW_ROWS))
}

/// `╔[<] TITLE [>]═══[+][x]╗` with working buttons.
#[allow(clippy::too_many_arguments)]
fn module_head(g: &mut Grid, hits: &mut Hits, x: usize, y: usize, w: usize, module: Module, slot: usize, can_add: bool, can_remove: bool) {
    let inner = w.saturating_sub(2);
    let controls = 6 + if can_add { 3 } else { 0 } + if can_remove { 3 } else { 0 };
    let title = if len(module.title()) + controls + 2 <= inner { module.title() } else { module.short() };
    g.put(x, y, "╔", AMBER);
    button(g, hits, x + 1, y, "[<]", AMBER, None, Action::Cycle(slot, -1));
    g.put(x + 4, y, &format!(" {title} "), BRIGHT);
    let next = x + 4 + len(title) + 2;
    button(g, hits, next, y, "[>]", AMBER, None, Action::Cycle(slot, 1));
    let fill = inner.saturating_sub(controls + len(title) + 2);
    g.put(next + 3, y, &"═".repeat(fill), AMBER);
    let mut at = next + 3 + fill;
    if can_add {
        button(g, hits, at, y, "[+]", AMBER, None, Action::AddSlot);
        at += 3;
    }
    if can_remove {
        button(g, hits, at, y, "[x]", AMBER, None, Action::RemoveSlot(slot));
    }
    g.put(x + w - 1, y, "╗", AMBER);
}

fn vu(g: &mut Grid, app: &App, x: usize, y: usize, cols: usize, rows: usize) {
    if rows == 0 {
        return;
    }
    let n = &app.needle;
    let bars = rows.saturating_sub(1).max(1);
    let w = cols.saturating_sub(7).div_euclid(2).max(1);
    let step = 1.0 / bars as f32;
    for i in 0..bars {
        let at = (bars - i) as f32 / bars as f32;
        let db = ((at - 1.0) * 36.0).round() as i32;
        if i % 3 == 0 {
            g.put(x, y + i, &format!("{db:4}"), DIM);
        }
        for (cx, level, peak) in [(x + 5, n.l, n.peak_l), (x + 5 + w + 1, n.r, n.peak_r)] {
            let lit = level >= at - step / 2.0;
            let is_peak = peak > 0.0 && peak >= at - step && peak < at + step * 0.01;
            let (ch, fg) = match (lit || is_peak, db > -6) {
                (true, true) => ("█", BRIGHT),
                (true, false) => ("█", AMBER),
                _ => ("░", OFF),
            };
            g.put(cx, y + i, &ch.repeat(w), fg);
        }
    }
    if rows >= 2 {
        g.put(x, y + bars, "  dB ", DIM);
        g.put(x + 5, y + bars, " L", AMBER);
        g.put(x + 5 + w + 1, y + bars, " R", AMBER);
    }
}

/// A rolling loudness waveform as text: one column per level, newest right, mirrored around the centre.
pub fn wave_rows(levels: &[f32], cols: usize, rows: usize) -> Vec<String> {
    let half = (rows * 2) as f32;
    let start = levels.len().saturating_sub(cols);
    let pad = cols.saturating_sub(levels.len());
    let heights: Vec<f32> = std::iter::repeat_n(0.0, pad).chain(levels[start..].iter().map(|v| wave_level(*v) * half)).collect();
    (0..rows)
        .map(|r| {
            heights
                .iter()
                .map(|h| {
                    let top = ((2 * r) as f32 + 0.5 - half / 2.0).abs() < h / 2.0;
                    let bottom = ((2 * r + 1) as f32 + 0.5 - half / 2.0).abs() < h / 2.0;
                    match (top, bottom) {
                        (true, true) => '█',
                        (true, false) => '▀',
                        (false, true) => '▄',
                        _ => ' ',
                    }
                })
                .collect()
        })
        .collect()
}

fn check(g: &mut Grid, x: usize, y: usize, on: bool) {
    g.put(x, y, if on { "[*]" } else { "[ ]" }, if on { BRIGHT } else { AMBER });
}

fn clock(unix: f64, with_date: bool) -> String {
    let Some(t) = chrono::DateTime::from_timestamp(unix as i64, 0) else { return "--:--".into() };
    let t = t.with_timezone(&chrono::Local);
    t.format(if with_date { "%d %b %H:%M" } else { "%H:%M" }).to_string()
}

fn history(g: &mut Grid, app: &mut App, hits: &mut Hits, r: Rect) {
    let top = scrolled(app, Scroll::Last, app.history.len(), r.h);
    hits.scrolls.push((r, Scroll::Last));
    if app.history.is_empty() {
        let text = if app.current.is_none() { copy::IDLE } else { copy::LOADING };
        g.put(r.x, r.y, &clip(&format!(" {text}"), r.w), DIM);
    }
    let rows: Vec<_> = app.history.iter().skip(top).take(r.h).cloned().collect();
    for (i, t) in rows.iter().enumerate() {
        let y = r.y + i;
        let time = t.started.map_or("--:--".to_string(), |s| clock(s, false));
        let right = if r.w >= 56 { t.votes.map_or(" ".to_string(), |v| format!(" ▲{} ▼{} ", v.up, v.down)) } else { " ".to_string() };
        let room = r.w.saturating_sub(len(&right) + 4);
        g.put(r.x, y, &clip(&format!(" {time} {} - {}", t.artist(), t.title()), room), AMBER);
        g.put(r.x + room, y, &right, DIM);
        check(g, r.x + room + len(&right), y, app.is_liked(t.track_id));
        hits.click(r.x, y, r.w, Action::LikeHistory(t.track_id));
    }
}

fn likes(g: &mut Grid, app: &mut App, hits: &mut Hits, r: Rect) {
    let top = scrolled(app, Scroll::Liked, app.saved.likes.len(), r.h);
    hits.scrolls.push((r, Scroll::Liked));
    if app.saved.likes.is_empty() {
        g.put(r.x, r.y, &clip(&format!(" {}", copy::NO_LIKES), r.w), DIM);
    }
    let rows: Vec<_> = app.saved.likes.iter().skip(top).take(r.h).cloned().collect();
    for (i, l) in rows.iter().enumerate() {
        let y = r.y + i;
        let found = l.apple_state == "found";
        let wide = r.w >= 72;
        let apple = if !wide { "[AM]" } else if found { copy::ON_APPLE } else if l.apple_state == "none" { copy::NOT_ON_APPLE } else { copy::APPLE_SEARCHING };
        let links = di::links(&format!("{} {}", l.artist, l.title));
        let tail = len(apple) + 1 + if wide { links.len() * 4 + 1 } else { 0 } + 4;
        g.put(r.x, y, &clip(&format!(" {} - {} ", l.artist, l.title), r.w.saturating_sub(tail)), AMBER);
        let mut at = r.x + r.w.saturating_sub(tail);
        button(g, hits, at, y, apple, if found { BRIGHT } else { DIM }, None, Action::OpenApple(l.id));
        at += len(apple) + 1;
        if wide {
            for (name, url) in links {
                button(g, hits, at, y, &format!("[{name}]"), AMBER, None, Action::Open(url));
                at += 4;
            }
            at += 1;
        }
        button(g, hits, at, y, "[x]", AMBER, None, Action::Unlike(l.id));
    }
}

fn heard(g: &mut Grid, app: &mut App, hits: &mut Hits, r: Rect) {
    if r.h == 0 {
        return;
    }
    let blink = (app.started.elapsed().as_secs_f32() * 2.0) as u64 % 2 == 0;
    let query = app.heard_query.clone();
    text_box(g, hits, r.x, r.y, r.w, copy::FIND, &query, false, copy::HEARD_SEARCH, app.focus == Focus::Heard, blink, Focus::Heard, Some(("[x]", Action::ClearHeard)));
    let q = query.to_lowercase();
    let found: Vec<_> = app.saved.heard.iter().rev().filter(|h| q.is_empty() || format!("{} {} {}", h.artist, h.title, h.channel).to_lowercase().contains(&q)).cloned().collect();
    let list = Rect { x: r.x, y: r.y + 1, w: r.w, h: r.h - 1 };
    let top = scrolled(app, Scroll::Heard, found.len(), list.h);
    hits.scrolls.push((list, Scroll::Heard));
    if found.is_empty() && list.h > 0 {
        g.put(list.x, list.y, &clip(&format!(" {}", copy::NO_HEARD), list.w), DIM);
    }
    for (i, h) in found.iter().skip(top).take(list.h).enumerate() {
        let y = list.y + i;
        let room = list.w.saturating_sub(5);
        g.put(list.x, y, &clip(&format!(" {} {} - {}", clock(h.date, true), h.artist, h.title), room), AMBER);
        check(g, list.x + room + 1, y, app.is_liked(h.track_id));
        hits.click(list.x, y, list.w, Action::LikeHeard(h.track_id));
    }
}

fn habits(g: &mut Grid, app: &mut App, hits: &mut Hits, r: Rect) {
    let secs = |app: &App, key: &str| app.saved.seconds.get(key).copied().unwrap_or(0);
    let mut ranked: Vec<_> = app.saved.channels.iter().filter(|c| secs(app, &c.key) >= 60).cloned().collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(secs(app, &c.key)));
    let total: u64 = app.saved.seconds.values().sum();
    let most = ranked.first().map_or(1, |c| secs(app, &c.key)).max(1);
    let name_w = (r.w / 2).clamp(10, 22);
    let bar_w = r.w.saturating_sub(name_w + 9).max(1);
    let shown = if hm(total).is_empty() { "0m".to_string() } else { hm(total) };
    g.put(r.x, r.y, &clip(&format!(" {shown} {} · {} {} · {} {}", copy::TOTAL, app.saved.heard.len(), copy::TRACKS, app.saved.likes.len(), copy::LIKED_WORD), r.w), BRIGHT);
    let list = Rect { x: r.x, y: r.y + 2, w: r.w, h: r.h.saturating_sub(2) };
    let top = scrolled(app, Scroll::Habits, ranked.len(), list.h);
    hits.scrolls.push((list, Scroll::Habits));
    if ranked.is_empty() && list.h > 0 {
        g.put(list.x, list.y, &clip(&format!(" {}", copy::NO_HABITS), list.w), DIM);
    }
    for (i, c) in ranked.iter().skip(top).take(list.h).enumerate() {
        let y = list.y + i;
        let s = secs(app, &c.key);
        let n = ((s as u128 * bar_w as u128 / most as u128) as usize).max(1).min(bar_w);
        g.put(list.x, y, &clip(&format!(" {}", c.name.to_uppercase()), name_w), AMBER);
        g.put(list.x + name_w, y, &"█".repeat(n), AMBER);
        g.put(list.x + name_w + n, y, &"░".repeat(bar_w - n), OFF);
        g.put(list.x + name_w + bar_w, y, &clip(&format!(" {}", hm(s)), 8), AMBER);
        hits.click(list.x, y, list.w, Action::Play(c.id));
    }
}

fn status_bar(g: &mut Grid, app: &App, hits: &mut Hits, y: usize) {
    let cols = g.cols;
    let state = app.current.as_ref().map_or("idle".to_string(), |c| format!("{} {}", if app.playing() { "playing" } else { "stopped" }, c.key));
    let key = if app.key().is_empty() { copy::NO_KEY_SHORT } else { copy::PREMIUM };
    let left = format!(" {state} │ {key} │ {} liked", app.saved.likes.len());
    let legend = copy::legend();
    g.put_on(0, y, &" ".repeat(cols), BG, Some(AMBER));
    g.put_on(0, y, &clip(&left, cols), BG, Some(AMBER));
    if cols >= len(&left) + len(&legend) + 10 {
        g.put_on(cols - len(&legend) - 8, y, &legend, BG, Some(AMBER));
    }
    if cols >= 8 {
        button(g, hits, cols - 8, y, "[-]", BG, Some(AMBER), Action::ZoomOut);
        button(g, hits, cols - 4, y, "[+]", BG, Some(AMBER), Action::ZoomIn);
    }
}

/// Paint the screen. Returns where the album art goes, in cells: x, y, w, h.
pub fn draw(g: &mut Grid, app: &mut App, hits: &mut Hits) -> Option<(usize, usize, usize, usize)> {
    let (cols, rows) = (g.cols, g.rows);
    if cols < 4 || rows < 3 {
        return None;
    }
    let side = 36.min((cols * 2 / 5).max(24)).min(cols);
    let right = cols - side;
    let body = rows - 1;
    boxed(g, 0, 0, side, body, Some(copy::APP));
    sidebar(g, app, hits, side, body);

    let now_box = (NOW_ROWS + 2).min(body);
    let mut art = None;
    if right >= 2 {
        boxed(g, side, 0, right, now_box, Some(copy::NOW));
        if now_box == NOW_ROWS + 2 {
            art = now_pane(g, app, hits, side + 1, 1, right - 2);
        }
    }

    let bay = body - now_box;
    if bay >= 4 && right >= 2 {
        let slots = app.saved.slots.clone();
        let fit = fit_modules(&slots, right);
        let used: usize = fit.iter().map(|(m, _)| m.min_cols()).sum();
        let room = slots.len() == fit.len() && slots.len() < MODULES.len() && used + Module::Last.min_cols() <= right;
        let mut x = side;
        for (i, (module, w)) in fit.iter().enumerate() {
            let w = (*w).min(cols.saturating_sub(x));
            if w < 2 {
                break;
            }
            boxed(g, x, now_box, w, bay, None);
            module_head(g, hits, x, now_box, w, *module, i, room && i + 1 == fit.len(), slots.len() > 1);
            let r = Rect { x: x + 1, y: now_box + 1, w: w - 2, h: bay - 2 };
            match module {
                Module::Vu => vu(g, app, r.x, r.y, r.w, r.h),
                Module::Wave => {
                    let levels = app.radio.meter.recent(r.w);
                    for (i, line) in wave_rows(&levels, r.w, r.h).iter().enumerate() {
                        g.put(r.x, r.y + i, line, AMBER);
                    }
                }
                Module::Last => history(g, app, hits, r),
                Module::Liked => likes(g, app, hits, r),
                Module::Heard => heard(g, app, hits, r),
                Module::Habits => habits(g, app, hits, r),
            }
            x += w;
        }
    }
    status_bar(g, app, hits, body);
    art
}

const BOLT: [&str; 13] = ["......XXXXXX", ".....XXXXXX.", "....XXXXXX..", "...XXXXXX...", "..XXXXXXXXXX", ".XXXXXXXXXX.", "......XXXX..", ".....XXXX...", "....XXXX....", "...XXXX.....", "..XXX.......", ".XX.........", "X..........."];

/// The boot screen at `age` seconds. Returns where the big logo goes, in cells: x and y, when it is showing.
pub fn boot(g: &mut Grid, age: f32) -> Option<(usize, usize)> {
    let shown = (age / 0.08) as usize;
    let (cols, rows) = (g.cols, g.rows);
    let y0 = rows.saturating_sub(BOLT.len() + 7) / 2;
    let x0 = cols.saturating_sub(BOLT[0].len()) / 2;
    for (i, row) in BOLT.iter().take(shown).enumerate() {
        g.put(x0, y0 + i, &row.replace('X', "█").replace('.', " "), AMBER);
    }
    let after = y0 + BOLT.len() + 1;
    if shown > BOLT.len() + 1 {
        g.put(cols.saturating_sub(len(copy::BOOT)) / 2, after + 3, copy::BOOT, DIM);
    }
    if shown > BOLT.len() + 1 && shown % 6 < 3 {
        g.put(cols / 2, after + 5, "█", AMBER);
    }
    (shown > BOLT.len()).then_some((cols.saturating_sub(len(copy::APP) * 3) / 2, after))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawn() -> (Grid, App, Hits) {
        let mut app = App::demo();
        let (mut g, mut hits) = (Grid::new(132, 47), Hits::default());
        draw(&mut g, &mut app, &mut hits);
        (g, app, hits)
    }

    fn cell_of(hits: &Hits, want: &Action) -> (usize, usize) {
        let (r, _) = hits.clicks.iter().rev().find(|(_, a)| a == want).unwrap_or_else(|| panic!("no hit for {want:?}"));
        (r.x, r.y)
    }

    #[test]
    fn screen_has_every_region_and_fits_the_grid() {
        let (g, ..) = drawn();
        let text = g.text();
        for needle in ["╔═[ BLIKSEM ]", "╔═[ NOW PLAYING ]", " VU ", " WAVEFORM ", " WHAT THE FUCK WAS THAT ", "▼ FAVOURITES (3)", "playing techno", "[ STOP ]", "[ LIKE THIS SHIT ]", "DI.FM TECHNO", "Not A Real Track"] {
            assert!(text.contains(needle), "missing {needle}\n{text}");
        }
        assert!(text.lines().all(|l| l.chars().count() <= 132));
    }

    /// Any window size and any module must draw without a panic: this is every shape the user can drag it to.
    #[test]
    fn no_size_panics_with_any_module_showing() {
        let mut app = App::demo();
        app.saved.heard.push(crate::app::Heard { date: 1_700_000_000.0, channel: "Techno".into(), track_id: 1, artist: "A".into(), title: "T".into(), art: None });
        for slots in [vec![Module::Vu, Module::Wave, Module::Last], vec![Module::Liked, Module::Heard, Module::Habits], vec![Module::Heard], vec![Module::Habits, Module::Vu]] {
            app.saved.slots = slots;
            for cols in 1..150 {
                for rows in 1..60 {
                    draw(&mut Grid::new(cols, rows), &mut app, &mut Hits::default());
                }
            }
        }
    }

    #[test]
    fn clicks_land_on_what_they_look_like() {
        let (g, mut app, hits) = drawn();
        let rows: Vec<String> = g.text().lines().map(String::from).collect();
        // The cell under every recorded button shows its label.
        let (x, y) = cell_of(&hits, &Action::Toggle);
        assert!(rows[y].chars().skip(x).collect::<String>().starts_with("[ STOP ]"), "{}", rows[y]);
        let (x, y) = cell_of(&hits, &Action::Like);
        assert!(rows[y].chars().skip(x).collect::<String>().starts_with("[ LIKE"), "{}", rows[y]);
        let (x, y) = cell_of(&hits, &Action::ZoomIn);
        assert!(rows[y].chars().skip(x).collect::<String>().starts_with("[+]"), "{}", rows[y]);

        // The favourite button sits on top of the row's play action.
        let (x, y) = cell_of(&hits, &Action::ToggleFav(37));
        assert_eq!(hits.action_at(x, y), Some(&Action::ToggleFav(37)));
        assert_eq!(hits.action_at(x + 5, y), Some(&Action::Play(37)));

        let (x, y) = cell_of(&hits, &Action::ToggleSection("techno"));
        let action = hits.action_at(x, y).unwrap().clone();
        app.act(action);
        assert!(app.saved.collapsed.contains("techno"));
        let (mut g2, mut h2) = (Grid::new(132, 47), Hits::default());
        draw(&mut g2, &mut app, &mut h2);
        assert!(g2.text().contains("► TECHNO"), "the arrow flips");
        assert!(!g2.text().contains("HARD TECHNO"), "its channels are gone");
    }

    #[test]
    fn every_module_flips_in_with_real_content() {
        let mut app = App::demo();
        app.saved.seconds.insert("techno".into(), 7200);
        app.saved.slots = vec![Module::Liked, Module::Heard, Module::Habits];
        let (mut g, mut hits) = (Grid::new(180, 47), Hits::default());
        draw(&mut g, &mut app, &mut hits);
        let t = g.text();
        assert!(t.contains("Sample Name - Concrete"), "likes\n{t}");
        assert!(t.contains(copy::NO_HEARD), "heard\n{t}");
        assert!(t.contains("2h0m on this channel") && t.contains("TECHNO"), "habits\n{t}");
        assert!(hits.clicks.iter().any(|(_, a)| matches!(a, Action::Unlike(1002))));
        assert!(hits.scrolls.iter().any(|(_, s)| *s == Scroll::Habits));
    }

    #[test]
    fn the_sidebar_scrolls_within_its_list_and_hop_reveals_the_channel() {
        let mut app = App::demo();
        let (mut g, mut hits) = (Grid::new(132, 30), Hits::default());
        app.scroll_by(Scroll::Sidebar, 999);
        draw(&mut g, &mut app, &mut hits);
        assert!(g.text().contains("VOCAL HOUSE"), "the bottom of the list is reachable\n{}", g.text());
        assert!(!g.text().contains("▼ FAVOURITES"));
        app.current = app.channel(36);
        app.reveal = true;
        let (mut g, mut hits) = (Grid::new(132, 30), Hits::default());
        draw(&mut g, &mut app, &mut hits);
        assert!(g.text().contains("▼ FAVOURITES"), "scrolled back to the current channel\n{}", g.text());
    }

    #[test]
    fn the_key_never_shows_on_screen() {
        let mut app = App::demo();
        app.saved.key = "supersecretkey123".into();
        app.focus = Focus::Key;
        let (mut g, mut hits) = (Grid::new(132, 47), Hits::default());
        draw(&mut g, &mut app, &mut hits);
        assert!(!g.text().contains("supersecret"));
        assert!(g.text().contains("key> ********"));
    }

    #[test]
    fn a_silent_meter_lights_nothing() {
        let mut app = App::demo();
        app.saved.slots = vec![Module::Vu];
        let (mut g, mut hits) = (Grid::new(132, 47), Hits::default());
        draw(&mut g, &mut app, &mut hits);
        assert!(g.cells.iter().filter(|c| c.ch == '█').count() == 0, "{}", g.text());
    }

    #[test]
    fn waveform_is_mirrored_and_newest_is_right() {
        let rows = wave_rows(&[0.0, 1.0], 3, 2);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].chars().collect::<Vec<_>>(), [' ', ' ', '█']);
        assert_eq!(rows[1].chars().collect::<Vec<_>>(), [' ', ' ', '█']);
    }

    #[test]
    fn wrap_keeps_to_the_width() {
        assert_eq!(wrap("one two three", 7, 3), ["one two", "three"]);
        assert_eq!(wrap("a b c d", 1, 2), ["a", "b"]);
        assert_eq!(wrap("toolongword", 4, 3), ["tool"]);
    }

    #[test]
    fn boot_animation_builds_up() {
        let mut early = Grid::new(132, 47);
        assert!(boot(&mut early, 0.2).is_none());
        let mut late = Grid::new(132, 47);
        assert!(boot(&mut late, 2.0).is_some());
        assert!(late.text().contains(copy::BOOT));
    }
}
