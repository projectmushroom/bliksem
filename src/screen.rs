//! The main screen, laid out cell for cell like the Swift `Screen`. Demo data only: this build never connects.
use crate::grid::*;

const SIDE: usize = 36;
const NOW_ROWS: usize = 12;
const ART_COLS: usize = 21;
const VU_COLS: usize = 20;

/// Fake loudness: a 135 BPM kick with some wobble, so meters move without audio.
fn level(t: f32, channel: f32) -> f32 {
    let beat = (t * 2.25).fract();
    let kick = (-beat * 4.0).exp();
    (0.52 + 0.3 * kick + 0.06 * (t * 7.3 + channel).sin() + 0.03 * (t * 31.0 + channel * 2.0).sin()).clamp(0.0, 1.0)
}

fn boxed(g: &mut Grid, x: usize, y: usize, w: usize, h: usize, head: Option<&str>) {
    let inner = w.saturating_sub(2);
    if let Some(title) = head {
        let bar = inner.saturating_sub(title.chars().count() + 5);
        g.put(x, y, &format!("╔═[ {title} ]{}╗", "═".repeat(bar)), AMBER);
    }
    for row in y + 1..y + h - 1 {
        g.put(x, row, "║", AMBER);
        g.put(x + w - 1, row, "║", AMBER);
    }
    g.put(x, y + h - 1, &format!("╚{}╝", "═".repeat(inner)), AMBER);
}

/// A module header: `╔[<] TITLE [>]═══[+][x]╗`.
fn module_head(g: &mut Grid, x: usize, y: usize, w: usize, title: &str, add: bool) {
    let inner = w - 2;
    let controls = 6 + 3 + if add { 3 } else { 0 };
    g.put(x, y, "╔[<]", AMBER);
    g.put(x + 4, y, &format!(" {title} "), BRIGHT);
    let after = x + 4 + title.chars().count() + 2;
    g.put(after, y, "[>]", AMBER);
    let fill = inner.saturating_sub(controls + title.chars().count() + 2);
    g.put(after + 3, y, &"═".repeat(fill), AMBER);
    g.put(after + 3 + fill, y, if add { "[+][x]╗" } else { "[x]╗" }, AMBER);
}

fn sidebar(g: &mut Grid, cols: usize, rows: usize) {
    let inner = cols - 2;
    g.put(1, 1, " find> ", AMBER);
    g.put(8, 1, "find a channel. type.", DIM);
    g.put(1, 2, &"─".repeat(inner), AMBER);
    let mut y = 3;
    let section = |g: &mut Grid, y: &mut usize, title: &str, count: usize| {
        g.put(1, *y, &format!(" ▼ {title} ({count})"), BRIGHT);
        *y += 1;
    };
    let row = |g: &mut Grid, y: &mut usize, fav: bool, name: &str, time: &str, on: bool, sub: Option<&str>| {
        if *y + 1 >= rows {
            return;
        }
        let bg = on.then_some(AMBER);
        let ink = if on { BG } else { AMBER };
        g.put_on(1, *y, &" ".repeat(inner), ink, bg);
        g.put_on(1, *y, if fav { "[*]" } else { "[ ]" }, if on { BG } else if fav { BRIGHT } else { DIM }, bg);
        g.put_on(4, *y, &format!(" {name}"), ink, bg);
        g.put_on(1 + inner - time.len() - 1, *y, time, ink, bg);
        *y += 1;
        if let Some(line) = sub {
            g.put_on(1, *y, &" ".repeat(inner), ink, bg);
            let clipped: String = line.chars().take(inner - 4).collect();
            g.put_on(5, *y, &clipped, if on { BG } else { DIM }, bg);
            *y += 1;
        }
    };
    section(g, &mut y, "FAVOURITES", 3);
    row(g, &mut y, true, "TECHNO", "4h12m", true, Some("Demo Artist - Not A Real Track"));
    row(g, &mut y, true, "DUB TECHNO", "1h40m", false, Some("Placeholder - Basement Pressure"));
    row(g, &mut y, true, "UNDERGROUND TECHNO", "52m", false, Some("Sample Name - Concrete (Dub)"));
    section(g, &mut y, "TECHNO", 6);
    for (name, time) in [("HARD TECHNO", "12m"), ("MINIMAL", ""), ("TECH HOUSE", "3m"), ("DEEP TECH", ""), ("PROGRESSIVE", ""), ("DETROIT HOUSE & TECHNO", "")] {
        row(g, &mut y, false, name, time, false, None);
    }
    section(g, &mut y, "EVERYTHING ELSE", 99);
    for name in ["00S CLUB HITS", "AMBIENT", "BIG BEAT", "BREAKS", "CHILLOUT", "DRUM AND BASS", "DUBSTEP", "ELECTRO HOUSE", "EUROBEAT", "GOA-PSY TRANCE", "HARDCORE", "IDM", "JUNGLE", "LOUNGE", "PSYCHILL", "TRANCE", "VOCAL HOUSE"] {
        row(g, &mut y, false, name, "", false, None);
    }
    g.put(1, rows - 3, &"─".repeat(inner), AMBER);
    g.put(1, rows - 2, " key> ", AMBER);
    g.put(7, rows - 2, "••••••••••••••••", AMBER);
    g.put(cols - 7, rows - 2, "[ OK ]", AMBER);
}

fn now_pane(g: &mut Grid, x: usize, y: usize, cols: usize) {
    let tx = x + ART_COLS + 1;
    g.put(tx, y, " DI.FM TECHNO", BRIGHT);
    g.put(tx, y + 2, " Not A Real Track", BRIGHT);
    g.put(tx, y + 5, " Demo Artist", AMBER);
    g.put(tx, y + 7, " ▲41 ▼2   4h12m on this channel", DIM);
    g.put(tx, y + 9, " [ STOP ]  [ LIKE THIS SHIT ]", AMBER);
    let _ = cols;
}

fn vu(g: &mut Grid, x: usize, y: usize, cols: usize, rows: usize, t: f32) {
    let (l, r) = (level(t, 0.0), level(t, 1.7));
    let (pl, pr) = ((level(t - 0.05, 0.0) + 0.06).min(1.0), (level(t - 0.05, 1.7) + 0.06).min(1.0));
    let bars = rows.saturating_sub(1).max(1);
    let w = (cols.saturating_sub(7) / 2).max(1);
    let step = 1.0 / bars as f32;
    for i in 0..bars {
        let at = (bars - i) as f32 / bars as f32;
        let db = ((at - 1.0) * 36.0).round() as i32;
        if i % 3 == 0 {
            g.put(x, y + i, &format!("{db:4}"), DIM);
        }
        for (cx, level, peak) in [(x + 5, l, pl), (x + 5 + w + 1, r, pr)] {
            let lit = level >= at - step / 2.0;
            let is_peak = peak >= at - step && peak < at + step * 0.01;
            let (ch, fg) = match (lit || is_peak, db > -6) {
                (true, true) => ("█", BRIGHT),
                (true, false) => ("█", AMBER),
                _ => ("░", OFF),
            };
            g.put(cx, y + i, &ch.repeat(w), fg);
        }
    }
    g.put(x, y + bars, "  dB ", DIM);
    g.put(x + 5, y + bars, " L", AMBER);
    g.put(x + 5 + w + 1, y + bars, " R", AMBER);
}

fn wave(g: &mut Grid, x: usize, y: usize, cols: usize, rows: usize, t: f32) {
    let half = (rows * 2) as f32;
    let heights: Vec<f32> = (0..cols).map(|i| ((level(t - (cols - i) as f32 * 0.05, 0.0) - 0.4) / 0.6).clamp(0.0, 1.0).powi(2) * half).collect();
    for r in 0..rows {
        let line: String = heights
            .iter()
            .map(|h| {
                let top = ((2 * r) as f32 + 0.5 - half / 2.0).abs() < h / 2.0;
                let bottom = ((2 * r + 1) as f32 + 0.5 - half / 2.0).abs() < h / 2.0;
                if top && bottom { '█' } else if top { '▀' } else if bottom { '▄' } else { ' ' }
            })
            .collect();
        g.put(x, y + r, &line, AMBER);
    }
}

fn last_tracks(g: &mut Grid, x: usize, y: usize, cols: usize, rows: usize) {
    let tracks = ["21:04 Demo Artist - Not A Real Track", "20:58 Placeholder - Basement Pressure", "20:51 Sample Name - Concrete (Dub)", "20:44 Nobody - Fake Kick Study", "20:37 Test Signal - 135", "20:30 Demo Artist - Second Fake", "20:23 Placeholder - Warehouse Tone"];
    for (i, t) in tracks.iter().take(rows).enumerate() {
        let line: String = format!(" {t}").chars().take(cols.saturating_sub(4)).collect();
        g.put(x, y + i, &line, AMBER);
        g.put(x + cols - 4, y + i, if i == 2 { "[*]" } else { "[ ]" }, if i == 2 { BRIGHT } else { DIM });
    }
}

/// Paint the screen at time `t`. Returns where the album art goes, in cells: x, y, w, h.
pub fn draw(g: &mut Grid, t: f32) -> (usize, usize, usize, usize) {
    let (cols, rows) = (g.cols, g.rows);
    let side = SIDE.min((cols * 2 / 5).max(24));
    let right = cols.saturating_sub(side).max(1);
    let body = rows - 1;
    boxed(g, 0, 0, side, body, Some("BLIKSEM"));
    sidebar(g, side, body);

    let now_box = NOW_ROWS + 2;
    boxed(g, side, 0, right, now_box, Some("NOW PLAYING"));
    now_pane(g, side + 1, 1, right - 2);

    let bay = body.saturating_sub(now_box);
    if bay >= 4 {
        let by = now_box;
        let inner_rows = bay - 2;
        let vu_w = VU_COLS;
        let flex = (right.saturating_sub(vu_w)) / 2;
        let last_w = right - vu_w - flex;
        boxed(g, side, by, vu_w, bay, None);
        module_head(g, side, by, vu_w, "VU", false);
        vu(g, side + 1, by + 1, vu_w - 2, inner_rows, t);

        boxed(g, side + vu_w, by, flex, bay, None);
        module_head(g, side + vu_w, by, flex, "WAVEFORM", false);
        wave(g, side + vu_w + 1, by + 1, flex - 2, inner_rows, t);

        boxed(g, side + vu_w + flex, by, last_w, bay, None);
        module_head(g, side + vu_w + flex, by, last_w, "LAST TRACKS", false);
        last_tracks(g, side + vu_w + flex + 1, by + 1, last_w - 2, inner_rows);
    }

    let left = " playing techno │ premium │ 7 liked";
    let legend = "SPACE play ↑↓ hop CMD+L like CMD+F find CMD+/- zoom ";
    g.put_on(0, body, &" ".repeat(cols), BG, Some(AMBER));
    g.put_on(0, body, left, BG, Some(AMBER));
    if cols >= left.chars().count() + legend.chars().count() + 10 {
        g.put_on(cols - legend.chars().count() - 8, body, legend, BG, Some(AMBER));
    }
    g.put_on(cols - 8, body, "[-] [+] ", BG, Some(AMBER));
    (side + 1, 1, ART_COLS, NOW_ROWS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_has_every_region_and_fits_the_grid() {
        let mut g = Grid::new(132, 47);
        draw(&mut g, 1.0);
        let text = g.text();
        for needle in ["╔═[ BLIKSEM ]", "╔═[ NOW PLAYING ]", " VU ", " WAVEFORM ", " LAST TRACKS ", "▼ FAVOURITES (3)", "playing techno", "[ STOP ]"] {
            assert!(text.contains(needle), "missing {needle}\n{text}");
        }
        assert!(text.lines().all(|l| l.chars().count() <= 132));
    }

    #[test]
    fn meters_move() {
        let (mut a, mut b) = (Grid::new(132, 47), Grid::new(132, 47));
        draw(&mut a, 1.0);
        draw(&mut b, 1.2);
        assert_ne!(a.text(), b.text());
    }

    /// Windows shrink: nothing may panic or draw outside at the minimum size.
    #[test]
    fn small_window_does_not_panic() {
        draw(&mut Grid::new(102, 34), 0.0);
    }
}
