<p align="center">
  <img src="docs/logo.png" alt="BLIKSEM" width="360">
</p>

<p align="center"><b>A DI.fm player on an amber CRT, because a normal music app is fucking boring.</b><br>
One app for Mac and Omarchy Linux, in Rust.</p>

![Oscilloscope and spectrum on Deep Tech](docs/scope-spectrum.jpg)

The whole screen is a grid of characters in the IBM VGA ROM font, painted into a framebuffer and pushed through a CRT
shader: curved glass, phosphor glow, scanlines, vignette. One colour only, like a real amber tube, so everything is the
same phosphor at a different brightness. Album art gets pixelated and turned amber too. All the copy lives in
`src/copy.rs`, so tune the swearing there.

You need a DI.fm premium listen key. DI allows one stream per key and the newest connection wins, so another device on
your key will kick this one. BLIKSEM reconnects once, then tells you, rather than starting a fight it can't win.

| | |
|---|---|
| ![Vectorscope, listening habits and VU meters](docs/vectorscope-habits-vu.jpg) | ![Last tracks and waveform](docs/tracks-waveform.jpg) |

![Full-width spectrum](docs/spectrum.jpg)

## Install

**Omarchy:**

```
omarchy plugin add https://github.com/projectmushroom/omarchy-bliksem.git --enable
```

That puts a bolt on your bar. Click it, paste your listen key with `Ctrl+V`, press `Enter`, done. It needs no root and
no Rust. Update with `omarchy plugin update projectmushroom.bliksem`, or just click `[ NEW SHIT x.y.z ]` in the player
when it shows up. Remove with `omarchy plugin remove projectmushroom.bliksem`.

That repo holds the plugin and prebuilt players for x86_64 and aarch64; `dev/publish-plugin.sh` builds each natively and
fills it from this one. To build from source instead, needing Rust and this private repo, run
`packaging/omarchy/install.sh`.

**Mac:** take the DMG from Releases. It is ad-hoc signed, so macOS will moan. After dragging it to Applications run
`xattr -d com.apple.quarantine /Applications/BLIKSEM.app`. First launch imports the old Swift app's key, favourites,
likes and habits. To build it yourself: `packaging/mac/build.sh`.

## Use

| | |
|---|---|
| `Space` | play or stop |
| `↑` `↓` | hop channels |
| `F`, `F11` | fullscreen, `Esc` to leave |
| `Cmd`/`Ctrl` `+` `-` `0` | zoom |
| `Cmd`/`Ctrl` `L` | like this shit |
| `Cmd`/`Ctrl` `F` | find a channel |
| `Cmd`/`Ctrl` `1`-`9` | favourites, then most played |
| `Cmd`/`Ctrl` `Q` | quit |

`Ctrl` on Linux, because `Super` belongs to Hyprland. Everything is clickable too. Closing the window leaves the music
playing, obviously.

**The bottom bay:** VU, waveform, spectrum, oscilloscope, vectorscope, last tracks, liked, heard and habits. `[<]` `[>]`
swaps a module, `[x]` kills it, and anything you killed waits on the rail above the bay as `[+ NAME]`.

**On the Omarchy bar:** click opens or hides the window, middle click plays or stops, scroll hops channels, right click
likes the track.

**From anywhere:** `bliksem toggle`, `next`, `prev`, `like`, `show`, `window`, `status`, `update`, `quit`. It also
speaks MPRIS, so media keys and Omarchy's media widget just work.

## Develop

```
cargo test                         # fake DI server, silent output: no key, no sound, no window
cargo run -- --demo                # the window with fake data, never connects
cargo run -- --snap out.png        # render the real screen and shader to a PNG with no window
cargo run -- --snap logo.png --boot 2 --size 560x440   # the boot bolt, as at the top of this page
cargo run -- --text                # the screen as plain text
cargo run -- --bench 10            # frame cost with no screen
dev/e2e-omarchy.sh                 # the full chain on Omarchy with no key and no sound, see the script
dev/test-launcher.sh               # the script the bar runs: right player, notification when it dies
dev/audio-glitches.sh              # counts clicks in a recording of any build
```

Use plain `cargo test`, not `--release`: release wraps integer overflow and hides the crashes.

## When something goes wrong

`bliksem report` prints the log, audio details and player state as Markdown for an agent to read, without the key. Run
`bliksem mark "pop"` the moment something sounds off, so the log shows exactly when. The log is at
`~/.local/state/bliksem.log` on Omarchy and `~/Library/Logs/BLIKSEM.log` on the Mac.
`packaging/omarchy/projectmushroom.bliksem/AGENTS.md` tells an agent on the machine what to do.

`BLIKSEM_DATA=/some/folder` keeps all saved data there, which also stops the Mac import. `--stream URL` plays that URL
instead of asking DI, `--capture out.png` saves what the window was handed every 2 seconds, and `--trace` logs clicks
and shortcuts. Don't run tests against the real stream without thinking: any connection with your key can stop your own
music, and that is a shit way to find a bug.

Saved data is `~/.config/bliksem/state.json` on Linux and `~/Library/Application Support/BLIKSEM/state.json` on the Mac.
It holds your key, so it is only readable by you.

The old Swift app is in git history and the v0.3.0 release.
