//! Everything the screen shows and everything a click or key can do. No window code, so tests can drive it.
use crate::copy;
use crate::di::{self, Channel, HistoryTrack, StreamError, Track};
use crate::meter::Ballistics;
use crate::radio::{Radio, Sink, State};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Arc;
use std::time::Instant;

pub const ZOOMS: [f32; 5] = [1.0, 1.5, 2.0, 2.5, 3.0];
pub const FAV_SECTION: &str = "favs";
const HEARD_CAP: usize = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Module {
    Vu,
    Wave,
    Last,
    Liked,
    Heard,
    Habits,
}

pub const MODULES: [Module; 6] = [Module::Vu, Module::Wave, Module::Last, Module::Liked, Module::Heard, Module::Habits];

impl Module {
    pub fn title(self) -> &'static str {
        match self {
            Module::Vu => copy::VU,
            Module::Wave => copy::WAVE,
            Module::Last => copy::LAST_ON,
            Module::Liked => copy::LIKES,
            Module::Heard => copy::HEARD,
            Module::Habits => copy::HABITS,
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Module::Vu => copy::VU_SHORT,
            Module::Wave => copy::WAVE,
            Module::Last => copy::LAST_ON_SHORT,
            Module::Liked => copy::LIKES_SHORT,
            Module::Heard => copy::HEARD_SHORT,
            Module::Habits => copy::HABITS,
        }
    }
    /// Meters stay narrow; lists share what is left.
    pub fn fixed_cols(self) -> Option<usize> {
        (self == Module::Vu).then_some(20)
    }
    pub fn min_cols(self) -> usize {
        self.fixed_cols().unwrap_or(28)
    }
}

/// Which modules fit in `cols`, and how wide each gets.
pub fn fit_modules(slots: &[Module], cols: usize) -> Vec<(Module, usize)> {
    let mut shown = vec![];
    let mut used = 0;
    for s in slots {
        if used + s.min_cols() <= cols {
            shown.push(*s);
            used += s.min_cols();
        }
    }
    if shown.is_empty() {
        shown.extend(slots.first());
    }
    let fixed: usize = shown.iter().filter_map(|m| m.fixed_cols()).sum();
    let flex = shown.iter().filter(|m| m.fixed_cols().is_none()).count();
    let room = cols.saturating_sub(fixed);
    let each = if flex > 0 { room / flex } else { 0 };
    let rest = room - each * flex;
    let last = shown.iter().rposition(|m| m.fixed_cols().is_none()).unwrap_or(shown.len().saturating_sub(1));
    shown.iter().enumerate().map(|(i, m)| (*m, m.fixed_cols().unwrap_or(each) + if i == last { rest } else { 0 })).collect()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Liked {
    pub id: i64,
    pub artist: String,
    pub title: String,
    pub channel: String,
    /// Seconds since 1970.
    pub date: f64,
    #[serde(default)]
    pub art: Option<String>,
    #[serde(default)]
    pub apple: Option<String>,
    /// "searching", "found" or "none".
    #[serde(default = "searching", rename = "appleState")]
    pub apple_state: String,
}

fn searching() -> String {
    "searching".into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Heard {
    pub date: f64,
    pub channel: String,
    #[serde(rename = "trackID")]
    pub track_id: i64,
    pub artist: String,
    pub title: String,
    #[serde(default)]
    pub art: Option<String>,
}

/// What survives a restart. Field names match the Swift app, so its data imports as is.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub channels: Vec<Channel>,
    #[serde(rename = "technoIDs")]
    pub techno_ids: Vec<i64>,
    pub likes: Vec<Liked>,
    pub heard: Vec<Heard>,
    pub seconds: HashMap<String, u64>,
    pub favs: Vec<i64>,
    pub collapsed: HashSet<String>,
    pub slots: Vec<Module>,
    pub key: String,
    pub zoom: f32,
    #[serde(rename = "lastChannel")]
    pub last_channel: Option<i64>,
}

impl Default for Saved {
    fn default() -> Self {
        Self {
            channels: vec![],
            techno_ids: vec![],
            likes: vec![],
            heard: vec![],
            seconds: HashMap::new(),
            favs: vec![],
            collapsed: HashSet::new(),
            slots: vec![Module::Vu, Module::Wave, Module::Last],
            key: String::new(),
            zoom: 1.0,
            last_channel: None,
        }
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
/// Seconds between 1970 and the 2001 epoch Swift's JSONEncoder uses for dates.
const SWIFT_EPOCH: f64 = 978_307_200.0;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
/// Builds saved data from the Swift app's stored values. `blob` returns the JSON stored under a key.
pub fn import_swift(blob: impl Fn(&str) -> Option<Vec<u8>>, key: Option<String>, zoom: Option<f64>, last: Option<i64>) -> Saved {
    fn read<T: serde::de::DeserializeOwned>(blob: &impl Fn(&str) -> Option<Vec<u8>>, name: &str) -> Option<T> {
        serde_json::from_slice(&blob(name)?).ok()
    }
    let mut s = Saved::default();
    s.channels = read(&blob, "channels").unwrap_or_default();
    s.techno_ids = read(&blob, "technoIDs").unwrap_or_default();
    s.likes = read(&blob, "likes").unwrap_or_default();
    s.heard = read(&blob, "heard").unwrap_or_default();
    s.seconds = read(&blob, "seconds").unwrap_or_default();
    s.favs = read(&blob, "favs").unwrap_or_default();
    s.collapsed = read::<HashSet<String>>(&blob, "collapsed").unwrap_or_default();
    if let Some(slots) = read::<Vec<Module>>(&blob, "slots").filter(|v| !v.is_empty()) {
        s.slots = slots;
    }
    s.likes.iter_mut().for_each(|l| l.date += SWIFT_EPOCH);
    s.heard.iter_mut().for_each(|h| h.date += SWIFT_EPOCH);
    s.key = key.unwrap_or_default();
    s.zoom = zoom.map_or(1.0, |z| (z as f32).max(1.0));
    s.last_channel = last;
    s
}

#[cfg(target_os = "macos")]
fn import_from_defaults() -> Option<Saved> {
    let out = std::process::Command::new("defaults").args(["export", "za.bliksem.app", "-"]).output().ok()?;
    let plist::Value::Dictionary(d) = plist::from_bytes::<plist::Value>(&out.stdout).ok()? else { return None };
    if d.is_empty() {
        return None;
    }
    let blob = |k: &str| d.get(k).and_then(|v| v.as_data()).map(<[u8]>::to_vec);
    let num = |v: &plist::Value| v.as_signed_integer().map(|i| i as f64).or_else(|| v.as_real());
    let key = d.get("key").and_then(|v| v.as_string()).map(String::from);
    let zoom = d.get("zoom").and_then(num);
    let last = d.get("lastChannel").and_then(|v| v.as_signed_integer());
    Some(import_swift(blob, key, zoom, last))
}

#[cfg(not(target_os = "macos"))]
fn import_from_defaults() -> Option<Saved> {
    None
}

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("BLIKSEM_DATA") {
        return d.into();
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/BLIKSEM")
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map_or(home.join(".config"), PathBuf::from).join("bliksem")
    }
}

/// Load saved data. A file that won't parse is kept beside it rather than overwritten by an empty one.
pub fn load(dir: &std::path::Path) -> Saved {
    let path = dir.join("state.json");
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            let _ = std::fs::rename(&path, dir.join("state.bad.json"));
            Saved::default()
        }),
        // First run: pick up the Swift app's data, unless this is a test folder.
        Err(_) if std::env::var_os("BLIKSEM_DATA").is_none() => import_from_defaults().unwrap_or_default(),
        Err(_) => Saved::default(),
    }
}

/// Write via a temp file so a crash can't leave half a file, private because it holds the listen key.
pub fn store(dir: &std::path::Path, saved: &Saved) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join("state.tmp");
    let data = serde_json::to_vec(saved).map_err(std::io::Error::other)?;
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        f.write_all(&data)?;
    }
    #[cfg(not(unix))]
    std::fs::write(&tmp, &data)?;
    std::fs::rename(&tmp, dir.join("state.json"))
}

#[derive(Clone, Debug)]
pub struct Section {
    pub id: &'static str,
    pub title: &'static str,
    pub channels: Vec<Channel>,
}

/// Results from background threads, applied on the main thread once per frame.
pub enum Event {
    Channels(Vec<Channel>),
    Techno(Vec<i64>),
    OnAir(Vec<(i64, Track)>),
    History(i64, Vec<HistoryTrack>),
    Art(String, image::RgbImage),
    Streams(i64, Result<Vec<String>, StreamError>),
    Apple(i64, Option<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    None,
    Find,
    Key,
    Heard,
}

/// Lists that scroll.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scroll {
    Sidebar,
    Last,
    Liked,
    Heard,
    Habits,
}

/// Everything a click can do. The screen records these with their cells as it draws.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Play(i64),
    ToggleFav(i64),
    ToggleSection(&'static str),
    Focus(Focus),
    ClearFind,
    ClearHeard,
    SubmitKey,
    Toggle,
    Like,
    LikeHistory(i64),
    LikeHeard(i64),
    Unlike(i64),
    OpenApple(i64),
    Open(String),
    Cycle(usize, i32),
    AddSlot,
    RemoveSlot(usize),
    ZoomIn,
    ZoomOut,
}

/// A key, with the window system taken out.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Text(String),
    Backspace,
    Enter,
    Escape,
    Up,
    Down,
    /// With CMD on the Mac, CTRL on Linux.
    Command(char),
}

/// What the window has to do after a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    Paste,
    Fullscreen,
    Quit,
}

pub struct App {
    pub saved: Saved,
    pub on_air: HashMap<i64, Track>,
    pub history: Vec<HistoryTrack>,
    pub current: Option<Channel>,
    pub status: String,
    pub query: String,
    pub heard_query: String,
    pub focus: Focus,
    pub radio: Radio,
    pub needle: Ballistics,
    pub scroll: HashMap<Scroll, usize>,
    /// Scroll the sidebar to the current channel on the next draw.
    pub reveal: bool,
    pub art: HashMap<String, Arc<image::RgbImage>>,
    pub started: Instant,
    /// Tests and renders never touch the network.
    pub offline: bool,
    dir: Option<PathBuf>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    wanted: bool,
    /// The demo shows a playing screen with no stream.
    pretend: bool,
    last_state: State,
    last_track: Option<i64>,
    art_pending: HashSet<String>,
    ticks: u64,
    last_tick: Instant,
    dirty: bool,
    last_save: Instant,
}

impl App {
    fn with(saved: Saved, radio: Radio, dir: Option<PathBuf>, offline: bool) -> Self {
        let (tx, rx) = channel();
        let now = Instant::now();
        Self {
            saved,
            on_air: HashMap::new(),
            history: vec![],
            current: None,
            status: String::new(),
            query: String::new(),
            heard_query: String::new(),
            focus: Focus::None,
            radio,
            needle: Ballistics::default(),
            scroll: HashMap::new(),
            reveal: false,
            art: HashMap::new(),
            started: now,
            offline,
            dir,
            tx,
            rx,
            wanted: false,
            pretend: false,
            last_state: State::Idle,
            last_track: None,
            art_pending: HashSet::new(),
            ticks: 0,
            last_tick: now,
            dirty: false,
            last_save: now,
        }
    }

    /// The real app: saved data, the audio device and the network.
    pub fn open() -> Self {
        let dir = data_dir();
        let saved = load(&dir);
        let mut app = Self::with(saved, Radio::new(Sink::Device), Some(dir), false);
        app.bg(|tx| {
            let channels = di::channels();
            if !channels.is_empty() {
                let _ = tx.send(Event::Channels(channels));
            }
            let techno = di::techno_ids();
            if !techno.is_empty() {
                let _ = tx.send(Event::Techno(techno));
            }
        });
        app.refresh();
        let last = app.saved.last_channel.and_then(|id| app.channel(id));
        // Pick up where you left off, but only if there is a key to play it with.
        if let Some(c) = last.filter(|_| !app.key().is_empty()) {
            app.play(&c);
        }
        app
    }

    /// Fake data with fake meters: renders, the --demo window and tests. Never connects.
    pub fn demo() -> Self {
        let names = [
            (36, "techno", "Techno"), (37, "dubtechno", "Dub Techno"), (38, "undergroundtechno", "Underground Techno"),
            (39, "hardtechno", "Hard Techno"), (40, "minimal", "Minimal"), (41, "techhouse", "Tech House"), (42, "deeptech", "Deep Tech"),
            (43, "progressive", "Progressive"), (44, "detroithousentechno", "Detroit House & Techno"), (50, "ambient", "Ambient"), (51, "bigbeat", "Big Beat"),
            (52, "breaks", "Breaks"), (53, "chillout", "Chillout"), (54, "drumandbass", "Drum and Bass"), (55, "dubstep", "Dubstep"), (56, "electrohouse", "Electro House"),
            (57, "eurobeat", "Eurobeat"), (58, "goa-psy", "Goa-Psy Trance"), (59, "hardcore", "Hardcore"), (60, "idm", "IDM"), (61, "jungle", "Jungle"),
            (62, "lounge", "Lounge"), (63, "psychill", "PsyChill"), (64, "trance", "Trance"), (65, "vocalhouse", "Vocal House"),
        ];
        let mut s = Saved::default();
        s.channels = names.iter().map(|(id, key, name)| Channel { id: *id, key: (*key).into(), name: (*name).into(), asset_url: Some("//demo/art.png".into()) }).collect();
        s.techno_ids = (36..=44).collect();
        s.favs = vec![36, 37, 38];
        s.key = "demo".into();
        s.seconds = [("techno", 15120), ("dubtechno", 6000), ("undergroundtechno", 3120), ("hardtechno", 720), ("techhouse", 180)].iter().map(|(k, v)| ((*k).to_string(), *v)).collect();
        let mut app = Self::with(s, Radio::new(Sink::Silent), None, true);
        let tracks = [("Demo Artist", "Not A Real Track"), ("Placeholder", "Basement Pressure"), ("Sample Name", "Concrete (Dub)"), ("Nobody", "Fake Kick Study"), ("Test Signal", "135"), ("Demo Artist", "Second Fake"), ("Placeholder", "Warehouse Tone")];
        app.history = tracks.iter().enumerate().map(|(i, (a, t))| HistoryTrack {
            track_id: 1000 + i as i64,
            display_artist: Some((*a).into()),
            display_title: Some((*t).into()),
            started: Some(1_793_990_000.0 - 400.0 * i as f64),
            art_url: None,
            votes: Some(di::Votes { up: 41 - i as i64, down: 2 }),
        }).collect();
        for (c, i) in [(36, 0), (37, 1), (38, 2)] {
            app.on_air.insert(c, Track::new(1000 + i, tracks[i as usize].0, tracks[i as usize].1));
        }
        app.saved.likes.push(Liked { id: 1002, artist: "Sample Name".into(), title: "Concrete (Dub)".into(), channel: "Techno".into(), date: 1_793_990_000.0, art: None, apple: Some("https://example.com".into()), apple_state: "found".into() });
        app.current = app.channel(36);
        app.pretend = true;
        let art = image::load_from_memory(include_bytes!("../assets/demo-art.png")).expect("demo art").to_rgb8();
        app.art.insert(di::img("//demo/art.png", 96), Arc::new(crate::grid::amber_art(&art)));
        app
    }

    fn bg(&self, f: impl FnOnce(Sender<Event>) + Send + 'static) {
        if self.offline {
            return;
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || f(tx));
    }

    pub fn key(&self) -> String {
        self.saved.key.trim().to_string()
    }

    pub fn channel(&self, id: i64) -> Option<Channel> {
        self.saved.channels.iter().find(|c| c.id == id).cloned()
    }

    /// The radio is making sound, or the demo pretends it is.
    pub fn playing(&self) -> bool {
        self.last_state == State::Playing || self.pretend
    }

    pub fn now_track(&self) -> Option<&Track> {
        self.current.as_ref().and_then(|c| self.on_air.get(&c.id))
    }

    pub fn now_art_path(&self) -> Option<&str> {
        let id = self.now_track()?.id;
        self.history.iter().find(|h| h.track_id == id)?.art_url.as_deref()
    }

    /// The address of the art that should be on screen, at the size the screen pixelates it to.
    pub fn art_url(&self) -> Option<String> {
        self.now_art_path().or(self.current.as_ref().and_then(|c| c.asset_url.as_deref())).map(|p| di::img(p, 96))
    }

    /// Favourites on top, DI's techno group next, everything else after. A search flattens it to the matches.
    pub fn sections(&self) -> Vec<Section> {
        let q = self.query.to_lowercase();
        if !q.is_empty() {
            let found = self.saved.channels.iter().filter(|c| c.name.to_lowercase().contains(&q) || c.key.contains(&q)).cloned().collect();
            return vec![Section { id: "results", title: copy::RESULTS, channels: found }];
        }
        let by_id: HashMap<i64, &Channel> = self.saved.channels.iter().map(|c| (c.id, c)).collect();
        let pick = |ids: &mut dyn Iterator<Item = &i64>| ids.filter_map(|id| by_id.get(id).map(|c| (*c).clone())).collect::<Vec<_>>();
        let pinned = pick(&mut self.saved.favs.iter());
        let techno = pick(&mut self.saved.techno_ids.iter().filter(|id| !self.saved.favs.contains(id)));
        let mut rest: Vec<Channel> = self.saved.channels.iter().filter(|c| !self.saved.favs.contains(&c.id) && !self.saved.techno_ids.contains(&c.id)).cloned().collect();
        rest.sort_by(|a, b| a.name.cmp(&b.name));
        vec![
            Section { id: FAV_SECTION, title: copy::FAVS, channels: pinned },
            Section { id: "techno", title: copy::TECHNO, channels: techno },
            Section { id: "rest", title: copy::REST, channels: rest },
        ]
    }

    /// What the arrow keys hop through: the channels you can see.
    pub fn visible(&self) -> Vec<Channel> {
        self.sections().into_iter().filter(|s| !self.saved.collapsed.contains(s.id)).flat_map(|s| s.channels).collect()
    }

    /// Number shortcuts and the bar menu: favourites first, then whatever you listen to most.
    pub fn top(&self) -> Vec<Channel> {
        let seconds = |c: &Channel| self.saved.seconds.get(&c.key).copied().unwrap_or(0);
        let mut listened: Vec<Channel> = self.saved.channels.iter().filter(|c| seconds(c) > 0 && !self.saved.favs.contains(&c.id)).cloned().collect();
        listened.sort_by_key(|c| std::cmp::Reverse(seconds(c)));
        let favs = self.saved.favs.iter().filter_map(|id| self.channel(*id));
        favs.chain(listened).take(9).collect()
    }

    pub fn is_fav(&self, id: i64) -> bool {
        self.saved.favs.contains(&id)
    }

    pub fn is_liked(&self, id: i64) -> bool {
        self.saved.likes.iter().any(|l| l.id == id)
    }

    pub fn toggle_fav(&mut self, id: i64) {
        match self.saved.favs.iter().position(|f| *f == id) {
            Some(i) => {
                self.saved.favs.remove(i);
            }
            None => self.saved.favs.push(id),
        }
        self.dirty = true;
    }

    pub fn toggle_collapsed(&mut self, id: &str) {
        if !self.saved.collapsed.remove(id) {
            self.saved.collapsed.insert(id.to_string());
        }
        self.dirty = true;
    }

    /// Flip a bay slot to the next module not already showing.
    pub fn cycle(&mut self, slot: usize, by: i32) {
        let n = MODULES.len() as i32;
        let Some(i) = self.saved.slots.get(slot).and_then(|m| MODULES.iter().position(|x| x == m)) else { return };
        for step in 1..n {
            let next = MODULES[(i as i32 + by * step).rem_euclid(n) as usize];
            if !self.saved.slots.contains(&next) {
                self.saved.slots[slot] = next;
                self.dirty = true;
                return;
            }
        }
    }

    pub fn add_slot(&mut self) {
        if let Some(free) = MODULES.iter().find(|m| !self.saved.slots.contains(m)) {
            self.saved.slots.push(*free);
            self.dirty = true;
        }
    }

    pub fn remove_slot(&mut self, i: usize) {
        if self.saved.slots.len() > 1 && i < self.saved.slots.len() {
            self.saved.slots.remove(i);
            self.dirty = true;
        }
    }

    pub fn zoom_in(&mut self) {
        if let Some(z) = ZOOMS.iter().find(|z| **z > self.saved.zoom) {
            self.saved.zoom = *z;
            self.dirty = true;
        }
    }

    pub fn zoom_out(&mut self) {
        if let Some(z) = ZOOMS.iter().rev().find(|z| **z < self.saved.zoom) {
            self.saved.zoom = *z;
            self.dirty = true;
        }
    }

    pub fn play(&mut self, c: &Channel) {
        self.current = Some(c.clone());
        self.last_track = None;
        self.history.clear();
        self.status = copy::LOADING.into();
        self.saved.last_channel = Some(c.id);
        self.dirty = true;
        self.reveal = true;
        let key = self.key();
        if key.is_empty() {
            self.status = copy::NO_KEY.into();
            return;
        }
        let (id, name) = (c.id, c.key.clone());
        self.bg(move |tx| {
            let _ = tx.send(Event::Streams(id, di::stream_urls(&name, &key)));
        });
    }

    /// Radio: stop drops the buffer, play reconnects live.
    pub fn stop(&mut self) {
        self.wanted = false;
        self.pretend = false;
        self.radio.stop();
        self.status.clear();
    }

    pub fn toggle(&mut self) {
        if self.wanted || self.pretend {
            self.stop();
        } else if let Some(c) = self.current.clone().or_else(|| self.visible().first().cloned()) {
            self.play(&c);
        }
    }

    pub fn hop(&mut self, by: i32) {
        let v = self.visible();
        if v.is_empty() {
            return;
        }
        let n = v.len() as i32;
        let i = v.iter().position(|c| Some(c.id) == self.current.as_ref().map(|x| x.id)).map_or(-1, |i| i as i32);
        self.play(&v[(i + by).rem_euclid(n) as usize]);
    }

    pub fn toggle_like(&mut self, id: i64, artist: &str, title: &str, channel: &str, art: Option<String>) {
        self.dirty = true;
        if let Some(i) = self.saved.likes.iter().position(|l| l.id == id) {
            self.saved.likes.remove(i);
            return;
        }
        self.saved.likes.insert(0, Liked { id, artist: artist.into(), title: title.into(), channel: channel.into(), date: unix_now(), art, apple: None, apple_state: searching() });
        let (artist, title) = (artist.to_string(), title.to_string());
        self.bg(move |tx| {
            let url = di::apple_find(di::isrc(id), &artist, &title);
            let _ = tx.send(Event::Apple(id, url));
        });
    }

    pub fn like_current(&mut self) {
        let (Some(t), Some(c)) = (self.now_track().cloned(), self.current.clone()) else { return };
        let art = self.now_art_path().map(String::from);
        self.toggle_like(t.id, t.artist(), t.title(), &c.name, art);
    }

    pub fn refresh(&mut self) {
        let channel = self.current.as_ref().map(|c| c.id);
        self.bg(move |tx| {
            if let Some(now) = di::now_playing() {
                let _ = tx.send(Event::OnAir(now));
            }
            if let Some(id) = channel {
                let _ = tx.send(Event::History(id, di::history(id)));
            }
        });
    }

    pub fn handle(&mut self, event: Event) {
        match event {
            Event::Channels(c) => {
                self.saved.channels = c;
                self.dirty = true;
            }
            Event::Techno(t) => {
                self.saved.techno_ids = t;
                self.dirty = true;
            }
            Event::OnAir(now) => {
                self.on_air = now.into_iter().collect();
                self.note_track();
            }
            Event::History(id, h) => {
                if self.current.as_ref().is_some_and(|c| c.id == id) {
                    self.history = h;
                    self.note_track();
                }
            }
            Event::Art(url, image) => {
                self.art_pending.remove(&url);
                // shortcut: the art cache just empties when it gets big, fine for a few hundred KB of thumbnails.
                if self.art.len() > 64 {
                    self.art.clear();
                }
                self.art.insert(url, Arc::new(image));
            }
            Event::Streams(id, result) => {
                if self.current.as_ref().is_none_or(|c| c.id != id) {
                    return; // hopped away while the playlist was loading
                }
                match result {
                    Ok(urls) => {
                        self.wanted = true;
                        self.radio.play(urls);
                        self.refresh();
                    }
                    Err(StreamError::BadKey) => self.status = copy::BAD_KEY.into(),
                    Err(StreamError::Network) => self.status = copy::DEAD.into(),
                }
            }
            Event::Apple(id, url) => {
                if let Some(l) = self.saved.likes.iter_mut().find(|l| l.id == id) {
                    l.apple_state = if url.is_some() { "found" } else { "none" }.into();
                    l.apple = url;
                    self.dirty = true;
                }
            }
        }
    }

    /// Add the track to the heard log once, when it first shows up while playing.
    fn note_track(&mut self) {
        let (Some(t), Some(c)) = (self.now_track().cloned(), self.current.clone()) else { return };
        if self.last_track == Some(t.id) {
            return;
        }
        self.last_track = Some(t.id);
        if self.playing() {
            let art = self.now_art_path().map(String::from);
            self.saved.heard.push(Heard { date: unix_now(), channel: c.name, track_id: t.id, artist: t.artist().into(), title: t.title().into(), art });
            let excess = self.saved.heard.len().saturating_sub(HEARD_CAP);
            self.saved.heard.drain(..excess);
            self.dirty = true;
        }
    }

    /// Radio state to words on the screen.
    fn radio_changed(&mut self, s: State) {
        self.status = match s {
            State::Idle | State::Playing => String::new(),
            State::Connecting | State::Buffering => copy::LOADING.into(),
            State::Reconnecting => copy::DROPPED.into(),
            State::Refused => copy::REFUSED.into(),
            State::NoAudio => copy::NO_AUDIO.into(),
        };
        if matches!(s, State::Refused | State::NoAudio) {
            self.wanted = false;
        }
    }

    /// Once per frame: background results, radio state, the one-second clock, saving.
    pub fn update(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            self.handle(event);
        }
        let state = self.radio.state();
        if state != self.last_state {
            self.last_state = state;
            self.radio_changed(state);
        }
        if self.last_tick.elapsed().as_secs_f32() >= 1.0 {
            self.last_tick += std::time::Duration::from_secs(1);
            self.ticks += 1;
            if state == State::Playing {
                if let Some(c) = &self.current {
                    *self.saved.seconds.entry(c.key.clone()).or_insert(0) += 1;
                    self.dirty = true;
                }
            }
            if self.ticks % 15 == 0 {
                self.refresh();
            }
        }
        if let Some(url) = self.art_url().filter(|u| !self.art.contains_key(u) && !self.art_pending.contains(u) && !self.offline) {
            self.art_pending.insert(url.clone());
            self.bg(move |tx| {
                if let Some(image) = di::bytes(&url).and_then(|b| image::load_from_memory(&b).ok()) {
                    let _ = tx.send(Event::Art(url, crate::grid::amber_art(&image.to_rgb8())));
                }
            });
        }
        if self.dirty && self.last_save.elapsed().as_secs() >= 5 {
            self.save();
        }
    }

    pub fn save(&mut self) {
        if let Some(dir) = &self.dir {
            if store(dir, &self.saved).is_ok() {
                self.dirty = false;
            }
        }
        self.last_save = Instant::now();
    }

    pub fn act(&mut self, action: Action) {
        // Clicking anything but a text box leaves the box, which also commits a typed key.
        if !matches!(action, Action::Focus(_) | Action::ClearFind | Action::ClearHeard) {
            self.focus = Focus::None;
        }
        match action {
            Action::Play(id) => {
                if let Some(c) = self.channel(id) {
                    self.play(&c);
                }
            }
            Action::ToggleFav(id) => self.toggle_fav(id),
            Action::ToggleSection(id) => self.toggle_collapsed(id),
            Action::Focus(f) => self.focus = f,
            Action::ClearFind => self.query.clear(),
            Action::ClearHeard => self.heard_query.clear(),
            Action::SubmitKey => self.submit_key(),
            Action::Toggle => self.toggle(),
            Action::Like => self.like_current(),
            Action::LikeHistory(id) => {
                if let Some(t) = self.history.iter().find(|t| t.track_id == id).cloned() {
                    let channel = self.current.as_ref().map(|c| c.name.clone()).unwrap_or_default();
                    self.toggle_like(id, t.artist(), t.title(), &channel, t.art_url.clone());
                }
            }
            Action::LikeHeard(id) => {
                if let Some(h) = self.saved.heard.iter().rev().find(|h| h.track_id == id).cloned() {
                    self.toggle_like(id, &h.artist, &h.title, &h.channel, h.art);
                }
            }
            Action::Unlike(id) => {
                self.saved.likes.retain(|l| l.id != id);
                self.dirty = true;
            }
            Action::OpenApple(id) => {
                if let Some(l) = self.saved.likes.iter().find(|l| l.id == id) {
                    let search = di::apple_search(&format!("{} {}", l.artist, l.title));
                    di::open_url(l.apple.as_deref().unwrap_or(&search));
                }
            }
            Action::Open(url) => di::open_url(&url),
            Action::Cycle(slot, by) => self.cycle(slot, by),
            Action::AddSlot => self.add_slot(),
            Action::RemoveSlot(i) => self.remove_slot(i),
            Action::ZoomIn => self.zoom_in(),
            Action::ZoomOut => self.zoom_out(),
        }
    }

    fn submit_key(&mut self) {
        self.saved.key = self.key();
        self.focus = Focus::None;
        self.dirty = true;
        self.save();
        if let Some(c) = self.current.clone().or_else(|| self.visible().first().cloned()).filter(|_| !self.wanted) {
            self.play(&c);
        }
    }

    fn field(&mut self) -> Option<&mut String> {
        match self.focus {
            Focus::Find => Some(&mut self.query),
            Focus::Key => Some(&mut self.saved.key),
            Focus::Heard => Some(&mut self.heard_query),
            Focus::None => None,
        }
    }

    pub fn paste(&mut self, text: &str) {
        let clean = text.trim().to_string();
        if let Some(f) = self.field() {
            f.push_str(&clean);
        }
    }

    pub fn input(&mut self, input: Input) -> Outcome {
        if let Input::Command(c) = input {
            return self.command(c);
        }
        if self.focus != Focus::None {
            match input {
                Input::Text(t) => {
                    let printable: String = t.chars().filter(|c| !c.is_control()).collect();
                    if let Some(f) = self.field() {
                        f.push_str(&printable);
                    }
                }
                Input::Backspace => {
                    if let Some(f) = self.field() {
                        f.pop();
                    }
                }
                Input::Enter => match self.focus {
                    Focus::Key => self.submit_key(),
                    Focus::Find => {
                        self.focus = Focus::None;
                        if let Some(c) = self.visible().first().cloned() {
                            self.play(&c);
                        }
                    }
                    _ => self.focus = Focus::None,
                },
                Input::Escape => {
                    // First Escape clears, second leaves. The key is never cleared by accident.
                    let leave = self.focus == Focus::Key || self.field().is_none_or(|f| f.is_empty());
                    if leave {
                        self.focus = Focus::None;
                    } else if let Some(f) = self.field() {
                        f.clear();
                    }
                }
                Input::Up => self.hop(-1),
                Input::Down => self.hop(1),
                Input::Command(_) => {}
            }
            return Outcome::Nothing;
        }
        match input {
            Input::Text(t) if t == " " => self.toggle(),
            Input::Text(t) if t.eq_ignore_ascii_case("f") => return Outcome::Fullscreen,
            Input::Up => self.hop(-1),
            Input::Down => self.hop(1),
            _ => {}
        }
        Outcome::Nothing
    }

    fn command(&mut self, c: char) -> Outcome {
        match c.to_ascii_lowercase() {
            'l' => self.like_current(),
            'f' => self.focus = Focus::Find,
            'p' => self.toggle(),
            'q' => return Outcome::Quit,
            'v' => return Outcome::Paste,
            '+' | '=' => self.zoom_in(),
            '-' => self.zoom_out(),
            '0' => {
                self.saved.zoom = 1.0;
                self.dirty = true;
            }
            d @ '1'..='9' => {
                let i = d as usize - '1' as usize;
                if let Some(ch) = self.top().get(i).cloned() {
                    self.play(&ch);
                }
            }
            _ => {}
        }
        Outcome::Nothing
    }

    /// Mouse wheel: `lines` is positive going down.
    pub fn scroll_by(&mut self, s: Scroll, lines: i32) {
        let v = self.scroll.entry(s).or_insert(0);
        *v = (*v as i64 + lines as i64).max(0) as usize;
    }
}

fn unix_now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// "4h12m", "52m", or empty under a minute.
pub fn hm(s: u64) -> String {
    match s {
        0..60 => String::new(),
        60..3600 => format!("{}m", s / 60),
        _ => format!("{}h{}m", s / 3600, s % 3600 / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chan(id: i64, name: &str) -> Channel {
        Channel { id, key: name.to_lowercase(), name: name.into(), asset_url: None }
    }

    fn app() -> App {
        let mut s = Saved::default();
        s.channels = vec![chan(1, "Techno"), chan(2, "Dub"), chan(3, "Zen"), chan(4, "Ambient")];
        s.techno_ids = vec![1, 2];
        s.key = "abc".into();
        App::with(s, Radio::new(Sink::Silent), None, true)
    }

    fn ids(v: &[Channel]) -> Vec<i64> {
        v.iter().map(|c| c.id).collect()
    }

    #[test]
    fn sections_pin_favourites_and_sort_the_rest() {
        let mut a = app();
        a.toggle_fav(2);
        let s = a.sections();
        assert_eq!(ids(&s[0].channels), [2]);
        assert_eq!(ids(&s[1].channels), [1], "a favourite leaves the techno group");
        assert_eq!(ids(&s[2].channels), [4, 3], "the rest sorted by name");
    }

    #[test]
    fn search_flattens_and_collapse_hides_from_the_arrows() {
        let mut a = app();
        a.query = "AMB".into();
        let s = a.sections();
        assert_eq!(s.len(), 1);
        assert_eq!(ids(&s[0].channels), [4]);
        a.query.clear();
        a.toggle_collapsed("techno");
        assert_eq!(ids(&a.visible()), [4, 3]);
        a.toggle_collapsed("techno");
        assert_eq!(ids(&a.visible()), [1, 2, 4, 3]);
    }

    #[test]
    fn hop_wraps_around() {
        let mut a = app();
        a.hop(1);
        assert_eq!(a.current.as_ref().unwrap().id, 1);
        a.hop(-1);
        assert_eq!(a.current.as_ref().unwrap().id, 3, "wraps to the last visible");
    }

    #[test]
    fn top_is_favourites_then_most_listened() {
        let mut a = app();
        a.toggle_fav(3);
        a.saved.seconds.insert("dub".into(), 50);
        a.saved.seconds.insert("ambient".into(), 90);
        assert_eq!(ids(&a.top()), [3, 4, 2]);
    }

    #[test]
    fn slots_cycle_past_what_is_showing_and_keep_one() {
        let mut a = app();
        a.cycle(0, 1);
        assert_eq!(a.saved.slots[0], Module::Liked, "wave and last are already showing");
        a.cycle(0, -1);
        assert_eq!(a.saved.slots[0], Module::Vu);
        a.add_slot();
        assert_eq!(a.saved.slots.len(), 4);
        for _ in 0..9 {
            a.remove_slot(0);
        }
        assert_eq!(a.saved.slots.len(), 1, "always keeps one");
    }

    #[test]
    fn modules_fit_the_width() {
        let s = [Module::Vu, Module::Wave, Module::Last];
        assert_eq!(fit_modules(&s, 96), [(Module::Vu, 20), (Module::Wave, 38), (Module::Last, 38)]);
        assert_eq!(fit_modules(&s, 97), [(Module::Vu, 20), (Module::Wave, 38), (Module::Last, 39)]);
        assert_eq!(fit_modules(&s, 50), [(Module::Vu, 20), (Module::Wave, 30)], "third doesn't fit");
        assert_eq!(fit_modules(&[Module::Last], 10).len(), 1, "always shows one");
    }

    #[test]
    fn no_key_says_so_instead_of_connecting() {
        let mut a = app();
        a.saved.key = "  ".into();
        a.hop(1);
        assert_eq!(a.status, copy::NO_KEY);
    }

    #[test]
    fn a_late_playlist_for_another_channel_is_ignored() {
        let mut a = app();
        a.current = a.channel(2);
        a.handle(Event::Streams(1, Ok(vec!["http://127.0.0.1:9/x".into()])));
        assert_eq!(a.radio.state(), State::Idle, "hopped away, so it must not start");
        a.handle(Event::Streams(2, Err(StreamError::BadKey)));
        assert_eq!(a.status, copy::BAD_KEY);
    }

    #[test]
    fn typing_goes_to_the_box_and_keys_go_to_the_player() {
        let mut a = app();
        assert_eq!(a.input(Input::Command('f')), Outcome::Nothing);
        assert_eq!(a.focus, Focus::Find);
        a.input(Input::Text("amb ".into()));
        assert_eq!(a.query, "amb ", "space is text in a box");
        a.input(Input::Backspace);
        a.input(Input::Enter);
        assert_eq!(a.focus, Focus::None);
        assert_eq!(a.current.as_ref().unwrap().id, 4, "Enter plays the first match");
        assert_eq!(a.input(Input::Text("f".into())), Outcome::Fullscreen);
        assert_eq!(a.input(Input::Command('v')), Outcome::Paste);
    }

    #[test]
    fn escape_clears_then_leaves_but_never_wipes_the_key() {
        let mut a = app();
        a.focus = Focus::Find;
        a.query = "x".into();
        a.input(Input::Escape);
        assert_eq!((a.query.as_str(), a.focus), ("", Focus::Find));
        a.input(Input::Escape);
        assert_eq!(a.focus, Focus::None);
        a.focus = Focus::Key;
        a.input(Input::Escape);
        assert_eq!((a.saved.key.as_str(), a.focus), ("abc", Focus::None));
    }

    #[test]
    fn paste_trims_and_control_characters_never_get_typed() {
        let mut a = app();
        a.saved.key.clear();
        a.focus = Focus::Key;
        a.paste("  abc123\n");
        assert_eq!(a.saved.key, "abc123");
        a.input(Input::Text("\u{8}\u{7f}x".into()));
        assert_eq!(a.saved.key, "abc123x", "no house glyph from backspace");
    }

    #[test]
    fn likes_toggle_and_heard_logs_once() {
        let mut a = app();
        a.current = a.channel(1);
        a.on_air.insert(1, Track::new(7, "A", "T"));
        a.like_current();
        assert!(a.is_liked(7));
        assert_eq!(a.saved.likes[0].apple_state, "searching");
        a.handle(Event::Apple(7, None));
        assert_eq!(a.saved.likes[0].apple_state, "none");
        a.like_current();
        assert!(!a.is_liked(7));

        a.last_state = State::Playing;
        a.note_track();
        a.note_track();
        assert_eq!(a.saved.heard.len(), 1);
        assert_eq!(a.saved.heard[0].title, "T");
    }

    #[test]
    fn the_status_line_follows_the_radio_and_a_kick_stops_wanting() {
        let mut a = app();
        a.wanted = true;
        a.radio_changed(State::Reconnecting);
        assert_eq!(a.status, copy::DROPPED);
        a.radio_changed(State::Refused);
        assert_eq!(a.status, copy::REFUSED);
        assert!(!a.wanted, "must not fight the other device");
    }

    #[test]
    fn saved_data_round_trips_privately() {
        let dir = std::env::temp_dir().join(format!("bliksem-test-{}", std::process::id()));
        let mut s = Saved::default();
        s.key = "secret".into();
        s.favs = vec![1];
        store(&dir, &s).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(dir.join("state.json")).unwrap().permissions().mode() & 0o777, 0o600, "it holds the key");
        }
        // SAFETY: only this test touches this variable's value for the folder it owns.
        unsafe { std::env::set_var("BLIKSEM_DATA", &dir) };
        let back = load(&dir);
        assert_eq!((back.key.as_str(), back.favs.as_slice()), ("secret", &[1][..]));

        std::fs::write(dir.join("state.json"), b"not json").unwrap();
        let bad = load(&dir);
        assert!(bad.key.is_empty());
        assert_eq!(std::fs::read(dir.join("state.bad.json")).unwrap(), b"not json", "a broken file is kept, not lost");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_swift_apps_data_imports_with_its_dates_fixed() {
        let likes = r#"[{"id":5,"artist":"A","title":"T","channel":"Techno","date":700000000.5,"art":null,"apple":"music://x","appleState":"found"}]"#;
        let heard = r#"[{"id":"7C1F6B9E-0000-0000-0000-000000000000","date":700000000,"channel":"Techno","trackID":5,"artist":"A","title":"T","art":null}]"#;
        let blob = |k: &str| match k {
            "likes" => Some(likes.as_bytes().to_vec()),
            "heard" => Some(heard.as_bytes().to_vec()),
            "seconds" => Some(br#"{"techno":90}"#.to_vec()),
            "favs" => Some(b"[36,37]".to_vec()),
            "collapsed" => Some(br#"["rest"]"#.to_vec()),
            "slots" => Some(br#"["vu","habits"]"#.to_vec()),
            "channels" => Some(br#"[{"id":36,"key":"techno","name":"Techno","asset_url":null}]"#.to_vec()),
            "technoIDs" => Some(b"[36]".to_vec()),
            _ => None,
        };
        let s = import_swift(blob, Some("k".into()), Some(1.5), Some(36));
        assert_eq!(s.likes[0].apple_state, "found");
        assert_eq!(s.likes[0].date, 700000000.5 + SWIFT_EPOCH);
        assert_eq!(s.heard[0].date, 700000000.0 + SWIFT_EPOCH);
        assert_eq!(s.seconds["techno"], 90);
        assert_eq!(s.favs, [36, 37]);
        assert!(s.collapsed.contains("rest"));
        assert_eq!(s.slots, [Module::Vu, Module::Habits]);
        assert_eq!((s.key.as_str(), s.zoom, s.last_channel), ("k", 1.5, Some(36)));
    }

    #[test]
    fn hm_formats() {
        assert_eq!(hm(59), "");
        assert_eq!(hm(3120), "52m");
        assert_eq!(hm(15120), "4h12m");
    }
}
