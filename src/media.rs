//! System-wide now playing: MPRIS on Linux, so the Omarchy bar and media keys work; Control Center and media keys on the Mac.
use crate::app::Event;
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig};
use std::sync::mpsc::Sender;

pub struct Media {
    controls: MediaControls,
    last: Option<Now>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Now {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub cover: Option<String>,
    pub playing: bool,
}

impl Media {
    pub fn new(tx: Sender<Event>) -> Option<Self> {
        let config = PlatformConfig { dbus_name: "bliksem", display_name: "BLIKSEM", hwnd: None };
        let mut controls = MediaControls::new(config).ok()?;
        controls.attach(move |e: MediaControlEvent| {
            let _ = tx.send(Event::Remote(e));
        }).ok()?;
        Some(Self { controls, last: None })
    }

    /// Only talks to the system when something changed: this is called every frame.
    pub fn update(&mut self, now: Now) {
        if self.last.as_ref() == Some(&now) {
            return;
        }
        let _ = self.controls.set_metadata(MediaMetadata {
            title: Some(&now.title),
            artist: Some(&now.artist),
            album: Some(&now.album),
            cover_url: now.cover.as_deref(),
            duration: None,
        });
        let _ = self.controls.set_playback(if now.playing { MediaPlayback::Playing { progress: None } } else { MediaPlayback::Stopped });
        self.last = Some(now);
    }
}
