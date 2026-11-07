# Diagnosing BLIKSEM

For an AI agent or anyone else on this machine looking into a BLIKSEM problem.

## What to run

```
~/.config/omarchy/plugins/projectmushroom.bliksem/bin/bliksem report
```

This prints a Markdown report: what each log event means, the player's state, the log added up, audio system details, and the last 300 log lines. It removes the listen key. Read it first, then write up what you find for the owner:

- what went wrong, with timestamps from the log
- the likely cause, and how sure you are
- anything you could not check

## When the problem comes and goes

Ask the owner to run this the moment they hear a pop or a glitch:

```
~/.config/omarchy/plugins/projectmushroom.bliksem/bin/bliksem mark "pop while switching"
```

It adds a `user_mark` line to the log. Compare it with the `audio_health` lines around it.

## Rules

- **Never print or share the listen key.** It is in `~/.config/bliksem/state.json` and in stream URLs. The report leaves it out, so don't go looking for it.
- **Don't start playback to test.** DI.fm allows one stream per key and the newest wins, so a test can stop the owner's music on another device. Ask first.
- **Don't change the system's audio settings** without asking. Report what you found.

## Where things are

| What | Where |
|---|---|
| Log | `~/.local/state/bliksem.log`, older lines in `bliksem.log.old` |
| Saved data | `~/.config/bliksem/state.json` (holds the key) |
| The player | `bin/<arch>/bliksem`, started through the `bin/bliksem` launcher |

Other commands: `bliksem status`, `bliksem update`, `bliksem show`, `bliksem window`, `bliksem toggle`, `bliksem quit`.
