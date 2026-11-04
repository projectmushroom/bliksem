# BLIKSEM

A DI.fm techno player for macOS that looks like your father's amber monitor.
Text-mode UI on a real 9×16 VGA ROM font, phosphor bloom, scanlines, curved glass.
Plays DI.fm streams, shows what every channel is playing, keeps your listening hours per channel,
logs everything you heard, lets you like a tune and find it on Apple Music, Beatport, Bandcamp, Discogs or YouTube.
System-wide Now Playing, media keys, menu bar.

## Build

    ./build.sh && open .build/BLIKSEM.app

Needs Xcode 26 on macOS 14 or later. No developer account needed, it's ad-hoc signed for your own Mac.

## Release

    ./release.sh 0.2.0

Builds a DMG, tags `v0.2.0` and publishes it as a GitHub release. The app is not notarized, so a DMG downloaded
from GitHub trips Gatekeeper once: `xattr -d com.apple.quarantine /Applications/BLIKSEM.app`, or right-click › Open.

## Test

    swift test

Runs headless and muted against a fake Icecast server inside the tests, so it never touches DI or your key. It covers
playing past the old 10 second drop, one socket per play, reconnecting after a cut, giving up when kicked, a refused key,
server fallback, channel hopping, and the meters.

To test against the real stream, also muted:

    DI_LISTEN_KEY=yourkey DI_SOAK=90 swift test --filter testRealStreamSoak

DI allows one stream per key and a new connection kicks the old one a few seconds later, so this stops your music on
any other device using that key.

To render the screen without connecting, using fake audio for the meters:

    BLIKSEM_SNAP=/tmp/screen.png .build/BLIKSEM.app/Contents/MacOS/Bliksem

## Key

Paste your DI.fm listen key in the `key>` field (di.fm › settings › listen key). Without it nothing plays: DI no longer serves free streams to third-party players.

## Keys

| key | does |
|---|---|
| click a channel, or ↑ ↓ | play it |
| space / ⌘P | stop and play |
| ⌘L | like what's on |
| ⌘F | find a channel |
| ⌘1 – ⌘9 | your favourites, then your most listened |
| ⌘+ ⌘- ⌘0 | zoom the whole screen |

## Credits

Font: Px437 IBM VGA 9x16 from [The Ultimate Oldschool PC Font Pack](https://int10h.org/oldschool-pc-fonts/) by VileR, CC BY-SA 4.0 (see FONT-LICENSE.txt).
Stream data from DI.fm's AudioAddict API, which is unofficial and may change.
