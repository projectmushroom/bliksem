# BLIKSEM

A DI.fm techno player for macOS that looks like your father's amber monitor.
Text-mode UI on a real 9×16 VGA ROM font, phosphor bloom, scanlines, curved glass.
Plays DI.fm streams, shows what every channel is playing, keeps your listening hours per channel,
logs everything you heard, lets you like a tune and find it on Apple Music, Beatport, Bandcamp, Discogs or YouTube.
System-wide Now Playing, media keys, menu bar.

## Build

    ./build.sh && open .build/BLIKSEM.app

Needs Xcode 26 on macOS 14 or later. No developer account needed, it's ad-hoc signed for your own Mac.

## Key

Paste your DI.fm listen key in the `key>` field (di.fm › settings › listen key). Without it nothing plays: DI no longer serves free streams to third-party players.

## Keys

| key | does |
|---|---|
| click a channel, or ↑ ↓ | play it |
| space / ⌘P | stop and play |
| ⌘L | like what's on |
| ⌘F | find a channel |
| ⌘1 – ⌘9 | your most listened channels |

## Credits

Font: Px437 IBM VGA 9x16 from [The Ultimate Oldschool PC Font Pack](https://int10h.org/oldschool-pc-fonts/) by VileR, CC BY-SA 4.0 (see FONT-LICENSE.txt).
Stream data from DI.fm's AudioAddict API, which is unofficial and may change.
