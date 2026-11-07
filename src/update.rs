//! Knows when a newer version exists. The public plugin's manifest is the one place every release passes through.
use crate::app::Event;
use std::sync::mpsc::Sender;
use std::time::Duration;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PLUGIN: &str = "projectmushroom.bliksem";
const MANIFEST: &str = "https://raw.githubusercontent.com/projectmushroom/omarchy-bliksem/main/manifest.json";
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

fn parts(v: &str) -> Option<Vec<u32>> {
    v.split('.').map(|p| p.parse().ok()).collect()
}

/// Server input: only a short plain version counts, because it ends up on screen and in a URL.
pub fn newer(remote: &str, ours: &str) -> bool {
    remote.len() <= 16 && matches!((parts(remote), parts(ours)), (Some(r), Some(o)) if r > o)
}

fn latest() -> Option<String> {
    let url = std::env::var("BLIKSEM_UPDATE_URL").unwrap_or(MANIFEST.into());
    let text = crate::di::text(&url).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&text).ok()?;
    manifest["version"].as_str().map(String::from)
}

/// Runs for the life of the app, so a player left on for days still finds out.
pub fn watch(tx: Sender<Event>) {
    loop {
        if let Some(v) = latest().filter(|v| newer(v, VERSION)) {
            if tx.send(Event::UpdateFound(v)).is_err() {
                return;
            }
        }
        std::thread::sleep(EVERY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_higher_plain_version_is_newer() {
        assert!(newer("0.5.1", "0.5.0"));
        assert!(newer("0.10.0", "0.9.9"), "numbers, not text");
        assert!(newer("1.0", "0.9.9"));
        assert!(!newer("0.5.0", "0.5.0"));
        assert!(!newer("0.4.9", "0.5.0"));
        assert!(!newer("0.6.0; rm -rf", "0.5.0"), "server input");
        assert!(!newer("", "0.5.0"));
        assert!(!newer("1.1.1.1.1.1.1.1.1.1.1", "0.5.0"), "too long to put on screen");
    }
}
