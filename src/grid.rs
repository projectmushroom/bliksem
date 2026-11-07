//! The whole UI is this: a grid of characters, painted into pixels.
use std::collections::HashMap;

pub type Rgb = [u8; 3];

/// One phosphor: every colour on screen is this one at some brightness, so nothing can drift to another yellow.
const fn shade(percent: u32) -> Rgb {
    [(255 * percent / 100) as u8, (168 * percent / 100) as u8, (31 * percent / 100) as u8]
}
pub const BRIGHT: Rgb = shade(100);
pub const AMBER: Rgb = shade(75);
pub const DIM: Rgb = shade(45);
/// The unlit VU segments.
pub const OFF: Rgb = shade(16);
pub const BG: Rgb = shade(5);

/// The phosphor at any brightness from 0 to 1, for gradients.
pub fn glow(v: f32) -> Rgb {
    BRIGHT.map(|c| (c as f32 * v.clamp(0.0, 1.0)) as u8)
}
pub const CELL_W: usize = 9;
pub const CELL_H: usize = 16;

#[derive(Clone, Copy)]
pub struct Cell {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
}

pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Cell>,
    /// Single font pixels painted over the cells, for what a character can't draw. Index, then colour.
    pub dots: Vec<(usize, Rgb)>,
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self { cols, rows, cells: vec![Cell { ch: ' ', fg: AMBER, bg: BG }; cols * rows], dots: vec![] }
    }

    /// One font pixel, clipped to the screen.
    pub fn dot(&mut self, x: usize, y: usize, c: Rgb) {
        if x < self.cols * CELL_W && y < self.rows * CELL_H {
            self.dots.push((y * self.cols * CELL_W + x, c));
        }
    }

    pub fn put(&mut self, x: usize, y: usize, s: &str, fg: Rgb) {
        self.put_on(x, y, s, fg, None);
    }

    /// Text clipped at the right edge; a background recolours the cells too.
    pub fn put_on(&mut self, x: usize, y: usize, s: &str, fg: Rgb, bg: Option<Rgb>) {
        if y >= self.rows {
            return;
        }
        for (i, ch) in s.chars().enumerate() {
            if x + i >= self.cols {
                break;
            }
            let cell = &mut self.cells[y * self.cols + x + i];
            cell.ch = ch;
            cell.fg = fg;
            if let Some(b) = bg {
                cell.bg = b;
            }
        }
    }

    /// The screen as plain text, so tests can read it.
    pub fn text(&self) -> String {
        self.cells.chunks(self.cols).map(|r| r.iter().map(|c| c.ch).collect::<String>().trim_end().to_string()).collect::<Vec<_>>().join("\n")
    }
}

/// The VGA ROM font as bits: one u16 per row, bit 0 is the leftmost pixel.
pub struct Font {
    glyphs: HashMap<char, [u16; CELL_H]>,
}

impl Font {
    pub fn load() -> Self {
        let font = fontdue::Font::from_bytes(&include_bytes!("../assets/Px437_IBM_VGA_9x16.ttf")[..], fontdue::FontSettings::default()).expect("bad font");
        let px = CELL_H as f32;
        let ascent = font.horizontal_line_metrics(px).expect("no metrics").ascent.round() as i32;
        let mut glyphs = HashMap::new();
        for &ch in font.chars().keys() {
            let (m, bitmap) = font.rasterize(ch, px);
            let mut rows = [0u16; CELL_H];
            for y in 0..m.height {
                for x in 0..m.width {
                    let (px_x, px_y) = (m.xmin + x as i32, ascent - m.height as i32 - m.ymin + y as i32);
                    // The outlines trace the ROM pixels, so coverage is 0 or full; anything else is rounding.
                    if bitmap[y * m.width + x] >= 128 && (0..CELL_W as i32).contains(&px_x) && (0..CELL_H as i32).contains(&px_y) {
                        rows[px_y as usize] |= 1 << px_x;
                    }
                }
            }
            glyphs.insert(ch, rows);
        }
        Self { glyphs }
    }

    /// The glyph for a character, or its closest match. CP437 has no ø or š, and artist names are full of them.
    pub fn glyph(&self, ch: char) -> Option<&[u16; CELL_H]> {
        self.glyphs.get(&ch).or_else(|| self.glyphs.get(&closest(ch))).or_else(|| self.glyphs.get(&'?'))
    }
}

/// Latin letters the ROM font lacks, as their plain base letter. Spaces and controls stay blank elsewhere.
fn closest(ch: char) -> char {
    match ch {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' | 'ǎ' => 'a',
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' => 'A',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' => 'C',
        'ď' | 'đ' | 'ð' => 'd',
        'Ď' | 'Đ' | 'Ð' => 'D',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => 'E',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' => 'G',
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' => 'I',
        'ķ' => 'k',
        'Ķ' => 'K',
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => 'l',
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' => 'L',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'Ñ' | 'Ń' | 'Ņ' | 'Ň' => 'N',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' => 'O',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'Ŕ' | 'Ŗ' | 'Ř' => 'R',
        'ś' | 'ŝ' | 'ş' | 'š' | 'ș' => 's',
        'Ś' | 'Ŝ' | 'Ş' | 'Š' | 'Ș' => 'S',
        'ß' => 's',
        'ţ' | 'ť' | 'ŧ' | 'ț' => 't',
        'Ţ' | 'Ť' | 'Ŧ' | 'Ț' => 'T',
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => 'U',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'Ý' | 'Ÿ' | 'Ŷ' => 'Y',
        'ź' | 'ż' | 'ž' => 'z',
        'Ź' | 'Ż' | 'Ž' => 'Z',
        '‘' | '’' | '‚' | 'ʼ' | '´' | '`' => '\'',
        '“' | '”' | '„' => '"',
        '–' | '—' | '−' => '-',
        '…' => '.',
        _ => ch,
    }
}

/// Paint the grid into RGBA, one pixel per font pixel.
pub fn paint(grid: &Grid, font: &Font, out: &mut Vec<u8>) {
    let width = grid.cols * CELL_W;
    out.resize(width * grid.rows * CELL_H * 4, 255);
    for (i, cell) in grid.cells.iter().enumerate() {
        let (cx, cy) = (i % grid.cols * CELL_W, i / grid.cols * CELL_H);
        let rows = font.glyph(cell.ch);
        for y in 0..CELL_H {
            let bits = rows.map_or(0, |r| r[y]);
            for x in 0..CELL_W {
                let c = if bits >> x & 1 == 1 { cell.fg } else { cell.bg };
                out[((cy + y) * width + cx + x) * 4..][..3].copy_from_slice(&c);
            }
        }
    }
    for (at, c) in &grid.dots {
        out[at * 4..][..3].copy_from_slice(c);
    }
}

/// Where the art goes until it arrives.
pub const ART_WAITING: Rgb = shade(20);

pub fn fill(out: &mut [u8], fb_width: usize, (x, y, w, h): (usize, usize, usize, usize), c: Rgb) {
    for dy in 0..h {
        for dx in 0..w {
            out[((y + dy) * fb_width + x + dx) * 4..][..3].copy_from_slice(&c);
        }
    }
}

/// Text at three times the font size, for the boot logo.
pub fn big_text(out: &mut [u8], fb_width: usize, font: &Font, x: usize, y: usize, text: &str, c: Rgb) {
    for (i, ch) in text.chars().enumerate() {
        let Some(rows) = font.glyph(ch) else { continue };
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..CELL_W {
                if bits >> col & 1 == 1 {
                    fill(out, fb_width, (x + i * CELL_W * 3 + col * 3, y + row * 3, 3, 3), c);
                }
            }
        }
    }
}

/// Greyscale, contrast 1.2, multiplied by amber: the same steps as the Swift `Art` view.
pub fn amber_art(image: &image::RgbImage) -> image::RgbImage {
    let mut out = image.clone();
    for p in out.pixels_mut() {
        let luma = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0;
        let v = ((luma - 0.5) * 1.2 + 0.5).clamp(0.0, 1.0);
        for i in 0..3 {
            p[i] = (v * BRIGHT[i] as f32) as u8;
        }
    }
    out
}

/// Scale to fill, crop the middle, no smoothing. `x`, `y`, `w`, `h` are in font pixels.
pub fn blit_art(out: &mut [u8], fb_width: usize, art: &image::RgbImage, (x, y, w, h): (usize, usize, usize, usize)) {
    let (sw, sh) = (art.width() as f32, art.height() as f32);
    let fill = (w as f32 / sw).max(h as f32 / sh);
    let (ox, oy) = ((sw - w as f32 / fill) / 2.0, (sh - h as f32 / fill) / 2.0);
    for dy in 0..h {
        for dx in 0..w {
            let sx = (ox + (dx as f32 + 0.5) / fill) as u32;
            let sy = (oy + (dy as f32 + 0.5) / fill) as u32;
            let p = art.get_pixel(sx.min(art.width() - 1), sy.min(art.height() - 1));
            out[((y + dy) * fb_width + x + dx) * 4..][..3].copy_from_slice(&p.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// If these pixels are wrong the whole look is wrong: blocks must fill the cell and borders must join up.
    #[test]
    fn font_fills_the_cell_and_borders_join() {
        let f = Font::load();
        assert!(f.glyph('█').unwrap().iter().all(|r| *r == 0x1ff), "full block fills 9x16");
        assert!(f.glyph('═').unwrap().iter().any(|r| *r == 0x1ff), "double line reaches both edges");
        assert!((0..16).all(|y| f.glyph('║').unwrap()[y] != 0), "double line reaches top and bottom");
        for ch in "╔╗╚╝─│▀▄░▲▼►↑↓".chars() {
            assert!(f.glyph(ch).is_some_and(|g| g.iter().any(|r| *r != 0)), "missing {ch}");
        }
        assert_eq!(f.glyph('▀').unwrap()[0], 0x1ff);
        assert_eq!(f.glyph('▀').unwrap()[15], 0);
        assert_eq!(f.glyph('▄').unwrap()[15], 0x1ff);
    }

    /// Real artist names must never show a hole: every letter draws something.
    #[test]
    fn no_letter_draws_blank() {
        let f = Font::load();
        for ch in "Grössand Sørensen Dvořák Łukasz Žiga Björk Müller Óscar Beyoncé Çağrı Ştefan Tiësto ’ – … ñ 北".chars().filter(|c| *c != ' ') {
            assert!(f.glyph(ch).is_some_and(|g| g.iter().any(|r| *r != 0)), "{ch} draws blank");
        }
        assert_eq!(f.glyph('ø'), f.glyph('o'));
        assert_eq!(f.glyph('北'), f.glyph('?'), "anything else is a visible question mark");
    }

    #[test]
    fn put_clips_and_paint_uses_the_colours() {
        let mut g = Grid::new(3, 1);
        g.put(1, 0, "█████", AMBER);
        g.put(0, 9, "off the screen", AMBER);
        assert_eq!(g.text(), " ██");
        let mut out = vec![];
        paint(&g, &Font::load(), &mut out);
        assert_eq!(out.len(), 27 * 16 * 4);
        assert_eq!(out[..3], BG);
        assert_eq!(out[9 * 4..9 * 4 + 3], AMBER);
    }

    #[test]
    fn every_shade_is_the_same_hue() {
        for c in [BRIGHT, AMBER, DIM, OFF, ART_WAITING, glow(0.33)] {
            let (g, b) = (c[1] as f32 / c[0] as f32, c[2] as f32 / c[0] as f32);
            assert!((g - 168.0 / 255.0).abs() < 0.03 && (b - 31.0 / 255.0).abs() < 0.03, "{c:?}");
        }
    }

    #[test]
    fn dots_paint_over_cells_and_clip() {
        let mut g = Grid::new(2, 1);
        g.dot(10, 3, BRIGHT);
        g.dot(18, 0, BRIGHT);
        g.dot(0, 16, BRIGHT);
        assert_eq!(g.dots.len(), 1, "off the grid is dropped");
        let mut out = vec![];
        paint(&g, &Font::load(), &mut out);
        assert_eq!(out[(3 * 18 + 10) * 4..][..3], BRIGHT);
    }

    #[test]
    fn art_is_amber_with_no_colour_left() {
        let mut img = image::RgbImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgb([0, 0, 255]));
        img.put_pixel(1, 0, image::Rgb([255, 255, 255]));
        let a = amber_art(&img);
        assert_eq!(a.get_pixel(1, 0).0, BRIGHT);
        let p = a.get_pixel(0, 0).0;
        assert!(p[0] >= p[1] && p[1] >= p[2], "still has blue: {p:?}");
    }
}
