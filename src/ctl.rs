//! One running player, and a small local socket so the bar and the command line can drive it.
use crate::app::Event;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

pub const COMMANDS: [&str; 8] = ["show", "window", "toggle", "next", "prev", "like", "status", "quit"];

fn path() -> PathBuf {
    if std::env::var_os("BLIKSEM_DATA").is_none() {
        if let Some(run) = std::env::var_os("XDG_RUNTIME_DIR") {
            return PathBuf::from(run).join("bliksem.sock");
        }
    }
    crate::app::data_dir().join("bliksem.sock")
}

/// Ask the running player. None means there isn't one.
pub fn send(command: &str) -> Option<String> {
    let mut s = UnixStream::connect(path()).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    writeln!(s, "{command}").ok()?;
    let mut reply = String::new();
    s.read_to_string(&mut reply).ok()?;
    Some(reply)
}

/// Become the running player. Commands arrive as events on the main thread.
pub fn serve(tx: Sender<Event>) {
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::remove_file(&path); // only called after `send` found nobody listening
    let Ok(listener) = UnixListener::bind(&path) else { return };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
            let Ok(clone) = stream.try_clone() else { continue };
            let mut line = String::new();
            // Local input, but still input: one short line, and only known words get through.
            let _ = BufReader::new(clone.take(32)).read_line(&mut line);
            let command = line.trim();
            if COMMANDS.contains(&command) {
                let _ = tx.send(Event::Ctl(command.to_string(), stream));
            }
        }
    });
}
