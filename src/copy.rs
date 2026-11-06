//! Every string the app shows. Tune the swearing here.
pub const APP: &str = "BLIKSEM";
pub const NOW: &str = "NOW PLAYING";
pub const FIND: &str = " find> ";
pub const SEARCH: &str = "find a channel. type.";
pub const TECHNO: &str = "TECHNO";
pub const FAVS: &str = "FAVOURITES";
pub const REST: &str = "EVERYTHING ELSE";
pub const RESULTS: &str = "FOUND";
pub const NO_FAVS: &str = "    hit [ ] on a channel to pin it";
pub const KEY_PROMPT: &str = " key> ";
pub const KEY_PLACEHOLDER: &str = "paste your listen key";
pub const KEY_HINT: &str = " di.fm > settings > listen key";
pub const IDLE: &str = "nothing playing. pick a channel.";
pub const NOTHING: &str = "nothing on. yet.";
pub const LOADING: &str = "connecting...";
pub const DEAD: &str = "stream died. fok.";
pub const BAD_KEY: &str = "that key is kak. check it.";
pub const DROPPED: &str = "stream dropped. reconnecting...";
pub const REFUSED: &str = "kicked off. your key is playing somewhere else.";
pub const NO_AUDIO: &str = "no audio device. what the fuck did you unplug.";
pub const NO_KEY: &str = "no key, no music. paste your fucking listen key.";
pub const STOP: &str = "[ STOP ]";
pub const PLAY: &str = "[ PLAY ]";
pub const LIKE: &str = "[ LIKE THIS SHIT ]";
pub const LIKED: &str = "[ LIKED ]";
pub const ON_APPLE: &str = "[ OPEN IN APPLE MUSIC ]";
pub const NOT_ON_APPLE: &str = "[ NOT ON APPLE. TYPICAL ]";
pub const APPLE_SEARCHING: &str = "[ asking apple... ]";
pub const LAST_ON: &str = "WHAT THE FUCK WAS THAT";
pub const LIKES: &str = "LIKED SHIT";
pub const HEARD: &str = "EVERYTHING YOU HEARD";
pub const VU: &str = "VU METER";
pub const VU_SHORT: &str = "VU";
pub const WAVE: &str = "WAVEFORM";
pub const HABITS: &str = "HABITS";
pub const TOTAL: &str = "listened";
pub const TRACKS: &str = "tracks";
pub const LIKED_WORD: &str = "liked";
pub const NO_HABITS: &str = "no habits yet. play something.";
pub const HEARD_SEARCH: &str = "when did i hear that...";
pub const LAST_ON_SHORT: &str = "LAST TRACKS";
pub const LIKES_SHORT: &str = "LIKED";
pub const HEARD_SHORT: &str = "HEARD";
pub const NO_LIKES: &str = "you've liked fuck all.";
pub const NO_HEARD: &str = "nothing heard yet. play something.";
pub const ON_CHANNEL: &str = "on this channel";
pub const NO_KEY_SHORT: &str = "no key";
pub const OK: &str = "[ OK ]";
pub const BOOT: &str = "DI.FM TECHNO TERMINAL";
pub const PREMIUM: &str = "premium";

/// SUPER belongs to Hyprland on Linux, so shortcuts use CTRL there and CMD on the Mac.
pub const MOD: &str = if cfg!(target_os = "macos") { "CMD" } else { "CTRL" };

pub fn legend() -> String {
    format!("SPACE play ↑↓ hop F full {MOD}+L like {MOD}+F find {MOD}+/- zoom ")
}
