//! DI.fm's public AudioAddict API and the iTunes search. Blocking calls: `app.rs` runs them on background threads.
//! Never log a stream or playlist URL, because it carries the listen key.
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Channel {
    pub id: i64,
    pub key: String,
    pub name: String,
    pub asset_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Track {
    pub id: i64,
    #[serde(default, rename = "display_artist")]
    artist: Option<String>,
    #[serde(default, rename = "display_title")]
    title: Option<String>,
}

impl Track {
    pub fn new(id: i64, artist: &str, title: &str) -> Self {
        Self { id, artist: Some(artist.into()), title: Some(title.into()) }
    }
    pub fn artist(&self) -> &str {
        self.artist.as_deref().unwrap_or("")
    }
    pub fn title(&self) -> &str {
        self.title.as_deref().unwrap_or("")
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Votes {
    pub up: i64,
    pub down: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct HistoryTrack {
    pub track_id: i64,
    #[serde(default)]
    pub display_artist: Option<String>,
    #[serde(default)]
    pub display_title: Option<String>,
    #[serde(default)]
    pub started: Option<f64>,
    #[serde(default)]
    pub art_url: Option<String>,
    #[serde(default)]
    pub votes: Option<Votes>,
}

impl HistoryTrack {
    pub fn artist(&self) -> &str {
        self.display_artist.as_deref().unwrap_or("")
    }
    pub fn title(&self) -> &str {
        self.display_title.as_deref().unwrap_or("")
    }
}

/// DI image paths come as "//cdn-images…png"; the CDN resizes with ?width=.
pub fn img(path: &str, width: u32) -> String {
    format!("https:{path}?width={width}")
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().into()
}

fn text(url: &str) -> Result<String, ureq::Error> {
    agent().get(url).call()?.body_mut().read_to_string()
}

fn api(path: &str) -> Option<Value> {
    serde_json::from_str(&text(&format!("https://api.audioaddict.com/v1/di/{path}")).ok()?).ok()
}

/// One bad element must not blank a whole list.
fn lenient<T: DeserializeOwned>(v: &Value) -> Vec<T> {
    v.as_array().map(|a| a.iter().filter_map(|e| serde_json::from_value(e.clone()).ok()).collect()).unwrap_or_default()
}

pub fn channels() -> Vec<Channel> {
    api("channels").map(|v| lenient(&v)).unwrap_or_default()
}

pub fn techno_ids() -> Vec<i64> {
    let Some(v) = api("channel_filters") else { return vec![] };
    techno_from_filters(&v)
}

fn techno_from_filters(v: &Value) -> Vec<i64> {
    let techno = v.as_array().and_then(|fs| fs.iter().find(|f| f["name"] == "Techno"));
    techno.map(|f| lenient::<Channel>(&f["channels"]).into_iter().map(|c| c.id).collect()).unwrap_or_default()
}

pub fn now_playing() -> Option<Vec<(i64, Track)>> {
    api("currently_playing").map(|v| now_from(&v))
}

fn now_from(v: &Value) -> Vec<(i64, Track)> {
    v.as_array()
        .map(|a| a.iter().filter_map(|e| Some((e["channel_id"].as_i64()?, serde_json::from_value(e["track"].clone()).ok()?))).collect())
        .unwrap_or_default()
}

pub fn history(channel: i64) -> Vec<HistoryTrack> {
    api(&format!("track_history/channel/{channel}")).map(|v| lenient(&v)).unwrap_or_default()
}

pub fn isrc(track: i64) -> Option<String> {
    api(&format!("tracks/{track}"))?["isrc"].as_str().map(String::from)
}

pub fn bytes(url: &str) -> Option<Vec<u8>> {
    agent().get(url).call().ok()?.body_mut().read_to_vec().ok()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StreamError {
    BadKey,
    Network,
}

/// Premium 320k servers for a channel. An invalid key gets plain text or an error back instead of a playlist.
pub fn stream_urls(channel: &str, key: &str) -> Result<Vec<String>, StreamError> {
    // Pasted input going into a URL: DI keys are plain alphanumerics, so anything else cannot be a key.
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(StreamError::BadKey);
    }
    let body = match text(&format!("https://listen.di.fm/premium_high/{channel}.pls?listen_key={key}")) {
        Ok(b) => b,
        Err(ureq::Error::StatusCode(_)) => return Err(StreamError::BadKey),
        Err(_) => return Err(StreamError::Network),
    };
    let urls = parse_pls(&body);
    if urls.is_empty() { Err(StreamError::BadKey) } else { Ok(urls) }
}

pub fn parse_pls(s: &str) -> Vec<String> {
    s.lines().filter(|l| l.starts_with("File")).filter_map(|l| l.split_once('=').map(|(_, u)| u.trim().to_string())).collect()
}

/// Percent-encode a search term.
pub fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn country() -> String {
    let locale = if cfg!(target_os = "macos") {
        std::process::Command::new("defaults").args(["read", "-g", "AppleLocale"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        std::env::var("LC_ALL").ok().or_else(|| std::env::var("LANG").ok())
    };
    region(&locale.unwrap_or_default())
}

/// "en_ZA.UTF-8" to "ZA". Goes into a URL, so only two letters pass.
fn region(locale: &str) -> String {
    let r = locale.split('.').next().unwrap_or("").rsplit(['_', '-']).next().unwrap_or("");
    if r.len() == 2 && r.chars().all(|c| c.is_ascii_alphabetic()) && locale.contains(['_', '-']) { r.to_uppercase() } else { "US".into() }
}

#[derive(Deserialize)]
struct AppleHit {
    #[serde(rename = "artistName")]
    artist: String,
    #[serde(rename = "trackViewUrl")]
    url: String,
}

fn apple_query(q: &str) -> Vec<AppleHit> {
    let Ok(body) = text(&format!("https://itunes.apple.com/{q}&country={}", country())) else { return vec![] };
    let Ok(v) = serde_json::from_str::<Value>(&body) else { return vec![] };
    lenient(&v["results"])
}

/// A link the Music app opens on the Mac and a browser opens on Linux.
fn music_link(url: &str) -> String {
    if cfg!(target_os = "macos") { url.replacen("https://", "music://", 1) } else { url.to_string() }
}

/// ISRC is exact when DI has it (rare); otherwise artist + title, first hit whose artist matches.
pub fn apple_find(isrc: Option<String>, artist: &str, title: &str) -> Option<String> {
    if let Some(hit) = isrc.and_then(|i| apple_query(&format!("lookup?isrc={}", enc(&i))).into_iter().next()) {
        return Some(music_link(&hit.url));
    }
    pick(&apple_query(&format!("search?term={}&entity=song&limit=5", enc(&format!("{artist} {title}")))), artist)
}

fn pick(hits: &[AppleHit], artist: &str) -> Option<String> {
    let a = artist.to_lowercase();
    hits.iter().find(|h| h.artist.to_lowercase().contains(&a) || a.contains(&h.artist.to_lowercase())).map(|h| music_link(&h.url))
}

pub fn apple_search(q: &str) -> String {
    let scheme = if cfg!(target_os = "macos") { "music" } else { "https" };
    format!("{scheme}://music.apple.com/{}/search?term={}", country().to_lowercase(), enc(q))
}

/// Shop and search links for a liked track: two-letter label and address.
pub fn links(q: &str) -> [(&'static str, String); 4] {
    let e = enc(q);
    [
        ("BE", format!("https://www.beatport.com/search?q={e}")),
        ("BA", format!("https://bandcamp.com/search?q={e}")),
        ("DI", format!("https://www.discogs.com/search/?q={e}")),
        ("YO", format!("https://www.youtube.com/results?search_query={e}")),
    ]
}

/// Hand a link to the system. No shell involved, so a track title can't inject anything.
pub fn open_url(url: &str) {
    let tool = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = std::process::Command::new(tool).arg(url).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pls_gives_every_server() {
        let pls = "[playlist]\nNumberOfEntries=2\nFile1=http://prem2.di.fm:80/techno_hi?k\nTitle1=x\nFile2= http://prem4.di.fm:80/techno_hi?k \nLength1=-1\n";
        assert_eq!(parse_pls(pls), ["http://prem2.di.fm:80/techno_hi?k", "http://prem4.di.fm:80/techno_hi?k"]);
        assert!(parse_pls("Invalid Listen Key").is_empty());
    }

    #[test]
    fn a_pasted_key_with_junk_never_reaches_the_network() {
        assert_eq!(stream_urls("techno", "abc&x=1"), Err(StreamError::BadKey));
        assert_eq!(stream_urls("techno", ""), Err(StreamError::BadKey));
        assert_eq!(stream_urls("techno", "abc def"), Err(StreamError::BadKey));
    }

    #[test]
    fn one_bad_element_does_not_blank_the_list() {
        let v = json!([{"id": 1, "key": "techno", "name": "Techno", "asset_url": null}, {"id": "broken"}, {"id": 2, "key": "dub", "name": "Dub"}]);
        let c: Vec<Channel> = lenient(&v);
        assert_eq!(c.iter().map(|c| c.id).collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn parses_the_api_shapes() {
        let f = json!([{"name": "House", "channels": [{"id": 9, "key": "h", "name": "H"}]}, {"name": "Techno", "channels": [{"id": 36, "key": "techno", "name": "Techno"}]}]);
        assert_eq!(techno_from_filters(&f), [36]);
        let n = json!([{"channel_id": 36, "track": {"id": 5, "display_artist": "A", "display_title": "T"}}, {"channel_id": 37, "track": null}]);
        let now = now_from(&n);
        assert_eq!(now.len(), 1);
        assert_eq!((now[0].0, now[0].1.artist(), now[0].1.title()), (36, "A", "T"));
    }

    #[test]
    fn search_terms_are_encoded() {
        assert_eq!(enc("A&B c"), "A%26B%20c");
        assert_eq!(enc("Tiësto"), "Ti%C3%ABsto");
    }

    #[test]
    fn only_a_clean_region_goes_in_the_url() {
        assert_eq!(region("en_ZA.UTF-8"), "ZA");
        assert_eq!(region("en-GB"), "GB");
        assert_eq!(region("C"), "US");
        assert_eq!(region(""), "US");
        assert_eq!(region("en_Z&.UTF-8"), "US");
    }

    #[test]
    fn apple_match_needs_the_right_artist() {
        let hits: Vec<AppleHit> = serde_json::from_value(json!([{"artistName": "Other", "trackViewUrl": "https://x/1"}, {"artistName": "Surgeon & Friend", "trackViewUrl": "https://x/2"}])).unwrap();
        let got = pick(&hits, "Surgeon").unwrap();
        assert!(got.ends_with("//x/2"), "{got}");
        assert_eq!(pick(&hits, "Nobody"), None);
    }
}
