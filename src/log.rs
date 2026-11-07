//! A log and a report an agent can read: `bliksem report` prints everything needed to diagnose a problem.
//! JSON lines, one event each. Only short facts go in: never a stream URL or anything else that could hold the key.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_BYTES: u64 = 2 * 1024 * 1024;
const TAIL_LINES: usize = 300;

const HOW_TO_READ: &str = "\
## How to read this

The log is JSON lines. A line that isn't JSON is stderr: a crash, or a message from a library.

| Event | Meaning |
|---|---|
| `start` | The player started. Version and platform. |
| `play` | A channel was picked. |
| `state` | Radio state changed: Connecting, Buffering, Playing, Reconnecting, Refused, NoAudio, Idle. |
| `audio_open` | The output opened: device, rate, channels, buffer. |
| `stream_open` | The server answered. `status` is HTTP. 401 or 403 means a bad key, or the key is busy elsewhere. |
| `stream_end` | The connection ended, and why. `lasted_s` is how long it ran. |
| `audio_health` | Written every 10 seconds, only when something went wrong. See below. |
| `output_lost` | The output really failed, so the player rebuilt it and reconnected. |
| `slow_frame` | One screen frame took over 200 ms. |
| `user_mark` | The person ran `bliksem mark` because they heard something. Compare its time with the lines around it. |
| `update_found`, `update_run` | Update check and update result. |
| `focus` | The bar asked to bring the window up. `ok: false` means the Hyprland call failed. |

`audio_health` fields, since the last one:
- `xruns`: the output ran out of audio before the callback delivered more. These are audible. Many of them point at scheduling or too small a buffer.
- `dry`: our 6 second queue ran empty, so the network fell behind. There is a fade and a gap, not a click.
- `clips`: decoded samples over full scale. Hot MP3s do this. Many of them can crackle.
- `decode_errors`: MP3 frames dropped. Each one is a small jump in the waveform, and can click.
- `late_ms`: the longest gap between audio callbacks, when over twice the expected time.

A pop with a `user_mark` and no `audio_health` nearby did not come from this app's path: look at PipeWire's `ERR` column and journal below.
";

pub fn path() -> PathBuf {
    if std::env::var_os("BLIKSEM_DATA").is_some() {
        return crate::app::data_dir().join("bliksem.log");
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    if cfg!(target_os = "macos") {
        home.join("Library/Logs/BLIKSEM.log")
    } else {
        std::env::var_os("XDG_STATE_HOME").map_or(home.join(".local/state"), PathBuf::from).join("bliksem.log")
    }
}

fn line(event: &str, fields: Value) -> String {
    let mut v = json!({ "t": chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false), "event": event });
    if let (Some(all), Value::Object(more)) = (v.as_object_mut(), fields) {
        all.extend(more);
    }
    v.to_string()
}

/// Add one event. Not for the audio callback: this touches the disk. Keeps the newest 2 to 4 MB across two files.
pub fn note(event: &str, fields: Value) {
    if cfg!(test) {
        return;
    }
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, path.with_extension("log.old"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{}", line(event, fields));
    }
}

/// Removes the key from text, however it got there, so a report is safe to hand to anyone.
fn scrub(text: &str, key: &str) -> String {
    let mut out = if key.len() >= 4 { text.replace(key, "[KEY]") } else { text.to_string() };
    let mut from = 0;
    while let Some(at) = out[from..].find("listen_key=") {
        let start = from + at + "listen_key=".len();
        let end = out[start..].find(|c: char| !c.is_ascii_alphanumeric()).map_or(out.len(), |n| start + n);
        out.replace_range(start..end, "[KEY]");
        from = start;
    }
    out
}

/// Adds up what the log says, so a reader sees the shape before the detail.
fn summary(lines: &[String]) -> String {
    let (mut events, mut health, mut other) = (BTreeMap::<String, u64>::new(), BTreeMap::<String, u64>::new(), 0);
    for l in lines {
        let Ok(v) = serde_json::from_str::<Value>(l) else {
            other += 1;
            continue;
        };
        *events.entry(v["event"].as_str().unwrap_or("?").to_string()).or_default() += 1;
        if v["event"] == "audio_health" {
            for field in ["xruns", "dry", "clips", "decode_errors"] {
                *health.entry(field.to_string()).or_default() += v[field].as_u64().unwrap_or(0);
            }
        }
    }
    format!("events: {events:?}\naudio problems added up: {health:?}\nnon-JSON lines (stderr): {other}\n")
}

/// Output of a system tool, or why there isn't any. Gives up after 5 seconds: a report must never hang.
fn tool(name: &str, args: &[&str]) -> String {
    let Ok(mut child) = Command::new(name).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
        return "(not available)\n".into();
    };
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if let Ok(Some(_)) = child.try_wait() {
            return child.wait_with_output().map_or(String::new(), |o| String::from_utf8_lossy(&o.stdout).into_owned());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    "(timed out)\n".into()
}

pub fn report() -> String {
    let path = path();
    let old = std::fs::read_to_string(path.with_extension("log.old")).unwrap_or_default();
    let now = std::fs::read_to_string(&path).unwrap_or_default();
    let lines: Vec<String> = old.lines().chain(now.lines()).map(String::from).collect();
    let tail = lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n");
    let mut r = format!("# BLIKSEM report\n\nversion {} on {} {}\nlog: {}\n\n{HOW_TO_READ}\n", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH, path.display());
    r += &format!("## Player right now\n\n```\n{}\n```\n\n", crate::ctl::send("status").unwrap_or("not running\n".into()).trim());
    r += &format!("## Whole log, added up\n\n```\n{}```\n\n", summary(&lines));
    let tools: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("sw_vers", &[]), ("system_profiler", &["SPAudioDataType"])]
    } else {
        &[("uname", &["-srm"]), ("hyprctl", &["version"]), ("pw-metadata", &["-n", "settings"]), ("wpctl", &["status"]), ("pw-top", &["-b", "-n", "2"]), ("journalctl", &["--user", "-u", "pipewire", "-u", "wireplumber", "-n", "40", "--no-pager"])]
    };
    r += "## System\n\n";
    for (name, args) in tools {
        r += &format!("`{name} {}`\n```\n{}```\n\n", args.join(" "), tool(name, args));
    }
    r += &format!("## Last {TAIL_LINES} log lines\n\n```\n{tail}\n```\n");
    let key = std::fs::read(crate::app::data_dir().join("state.json")).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v["key"].as_str().map(String::from)).unwrap_or_default();
    scrub(&r, &key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_json_with_a_time_and_an_event() {
        let v: Value = serde_json::from_str(&line("audio_health", json!({ "xruns": 3 }))).unwrap();
        assert_eq!(v["event"], "audio_health");
        assert_eq!(v["xruns"], 3);
        assert!(v["t"].as_str().unwrap().contains('T'));
    }

    #[test]
    fn the_key_never_survives_a_report() {
        assert_eq!(scrub("a supersecret b", "supersecret"), "a [KEY] b");
        assert_eq!(scrub("http://x/y?listen_key=abc123&z=1 and listen_key=def", ""), "http://x/y?listen_key=[KEY]&z=1 and listen_key=[KEY]");
        assert_eq!(scrub("short keys are left to the url rule: abc", "abc"), "short keys are left to the url rule: abc");
    }

    #[test]
    fn the_summary_adds_up_health_and_counts_stderr() {
        let lines = [line("audio_health", json!({ "xruns": 2, "dry": 1 })), line("audio_health", json!({ "xruns": 3 })), line("play", json!({})), "thread 'x' panicked".to_string()];
        let s = summary(&lines);
        assert!(s.contains("\"xruns\": 5") && s.contains("\"dry\": 1"), "{s}");
        assert!(s.contains("\"play\": 1") && s.contains("(stderr): 1"), "{s}");
    }

    #[test]
    fn a_missing_tool_is_reported_not_fatal() {
        assert_eq!(tool("bliksem-no-such-tool", &[]), "(not available)\n");
        assert_eq!(tool("sh", &["-c", "echo hi"]), "hi\n");
    }
}
