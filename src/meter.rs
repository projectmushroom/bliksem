//! Live levels for the VU meter and the waveform. Written from the audio callback, read by the screen.
use std::sync::Mutex;
use std::time::Instant;

const RING: usize = 512;

#[derive(Clone, Copy, Default, Debug)]
pub struct Levels {
    pub rms_l: f32,
    pub rms_r: f32,
    pub peak_l: f32,
    pub peak_r: f32,
}

struct Inner {
    now: Levels,
    last: Option<Instant>,
    blocks: usize,
    ring: [f32; RING],
}

pub struct Meter {
    inner: Mutex<Inner>,
}

impl Default for Meter {
    fn default() -> Self {
        Self { inner: Mutex::new(Inner { now: Levels::default(), last: None, blocks: 0, ring: [0.0; RING] }) }
    }
}

impl Meter {
    /// One finished 50ms block. Skips rather than waits: the audio thread must never block on the screen.
    pub fn push(&self, block: Levels) {
        let Ok(mut m) = self.inner.try_lock() else { return };
        m.now = block;
        m.last = Some(Instant::now());
        let at = m.blocks % RING;
        m.ring[at] = block.rms_l.max(block.rms_r);
        m.blocks += 1;
    }

    /// Fake kick-and-floor audio for renders with no stream. `t` is seconds.
    pub fn push_demo(&self, t: f32) {
        let v = 0.08 + 0.55 * (1.0 - (t / 0.46).fract()).powi(3);
        self.push(Levels { rms_l: v, rms_r: v * 0.9, peak_l: v * 1.3, peak_r: v * 1.2 });
    }

    /// Current levels, falling to silence when audio stops arriving.
    pub fn read(&self) -> Levels {
        let m = self.inner.lock().unwrap();
        let fade = m.last.map_or(0.0, |l| (1.0 - l.elapsed().as_secs_f32() / 0.5).max(0.0));
        Levels { rms_l: m.now.rms_l * fade, rms_r: m.now.rms_r * fade, peak_l: m.now.peak_l * fade, peak_r: m.now.peak_r * fade }
    }

    /// The newest `count` block levels, oldest first.
    pub fn recent(&self, count: usize) -> Vec<f32> {
        let m = self.inner.lock().unwrap();
        let live = m.last.is_some_and(|l| l.elapsed().as_secs_f32() < 0.5);
        let n = count.min(m.blocks).min(RING);
        (0..n).map(|i| if live { m.ring[(m.blocks - n + i) % RING] } else { 0.0 }).collect()
    }
}

/// Collects samples into 50ms blocks inside the audio callback.
pub struct Blocks {
    size: usize,
    n: usize,
    sum_l: f32,
    sum_r: f32,
    peak_l: f32,
    peak_r: f32,
}

impl Blocks {
    pub fn new(rate: u32) -> Self {
        Self { size: (rate as usize / 20).max(1), n: 0, sum_l: 0.0, sum_r: 0.0, peak_l: 0.0, peak_r: 0.0 }
    }

    pub fn add(&mut self, l: f32, r: f32, meter: &Meter) {
        self.sum_l += l * l;
        self.sum_r += r * r;
        self.peak_l = self.peak_l.max(l.abs());
        self.peak_r = self.peak_r.max(r.abs());
        self.n += 1;
        if self.n == self.size {
            let n = self.n as f32;
            meter.push(Levels { rms_l: (self.sum_l / n).sqrt(), rms_r: (self.sum_r / n).sqrt(), peak_l: self.peak_l, peak_r: self.peak_r });
            (self.n, self.sum_l, self.sum_r, self.peak_l, self.peak_r) = (0, 0.0, 0.0, 0.0, 0.0);
        }
    }
}

/// Linear amplitude to 0..=1 for the VU meter: -36 dB to 0 dB.
pub fn meter_level(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    ((20.0 * x.log10() + 36.0) / 36.0).clamp(0.0, 1.0)
}

/// Linear amplitude to 0..=1 for the waveform. DI Techno measured between about -18 and -6 dBFS, so a -36 dB floor
/// would draw a flat bar. Squared so kicks stand out.
pub fn wave_level(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let n = ((20.0 * x.log10() + 24.0) / 19.0).clamp(0.0, 1.0);
    n * n
}

/// Needle physics: fast up, slow down, a peak that hangs.
pub struct Ballistics {
    pub l: f32,
    pub r: f32,
    pub peak_l: f32,
    pub peak_r: f32,
    at: Instant,
    held_l: Instant,
    held_r: Instant,
}

impl Default for Ballistics {
    fn default() -> Self {
        let now = Instant::now();
        Self { l: 0.0, r: 0.0, peak_l: 0.0, peak_r: 0.0, at: now, held_l: now, held_r: now }
    }
}

impl Ballistics {
    pub fn step(&mut self, meter: &Meter) {
        let v = meter.read();
        let now = Instant::now();
        let dt = now.duration_since(self.at).as_secs_f32();
        self.at = now;
        self.l = approach(self.l, meter_level(v.rms_l), dt);
        self.r = approach(self.r, meter_level(v.rms_r), dt);
        hold(&mut self.peak_l, meter_level(v.peak_l), &mut self.held_l, now, dt);
        hold(&mut self.peak_r, meter_level(v.peak_r), &mut self.held_r, now, dt);
    }
}

fn approach(a: f32, b: f32, dt: f32) -> f32 {
    a + (b - a) * (1.0 - (-dt / if b > a { 0.05 } else { 0.3 }).exp())
}

fn hold(peak: &mut f32, v: f32, held: &mut Instant, now: Instant, dt: f32) {
    if v >= *peak {
        *peak = v;
        *held = now;
    } else if now.duration_since(*held).as_secs_f32() > 1.2 {
        *peak = v.max(*peak - dt * 0.4);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_match_the_swift_app() {
        assert_eq!(meter_level(0.0), 0.0);
        assert_eq!(meter_level(1.0), 1.0);
        assert!((meter_level(0.1) - 16.0 / 36.0).abs() < 1e-4, "-20 dB");
        assert!(wave_level(10f32.powf(-24.0 / 20.0)) < 1e-4, "floor at -24 dB");
        assert!((wave_level(10f32.powf(-5.0 / 20.0)) - 1.0).abs() < 1e-3, "top at -5 dB");
    }

    #[test]
    fn blocks_measure_a_full_scale_square_wave() {
        let (meter, mut blocks) = (Meter::default(), Blocks::new(44100));
        for i in 0..2205 {
            let s = if i % 2 == 0 { 0.5 } else { -0.5 };
            blocks.add(s, s / 2.0, &meter);
        }
        let v = meter.read();
        assert!((v.rms_l - 0.5).abs() < 0.01 && (v.rms_r - 0.25).abs() < 0.01 && (v.peak_l - 0.5).abs() < 0.01, "{v:?}");
        assert_eq!(meter.recent(9).len(), 1);
    }

    #[test]
    fn needle_rises_fast_then_falls_slowly() {
        let (meter, mut needle) = (Meter::default(), Ballistics::default());
        meter.push(Levels { rms_l: 0.5, rms_r: 0.5, peak_l: 0.5, peak_r: 0.5 });
        needle.at = Instant::now() - std::time::Duration::from_millis(300);
        needle.step(&meter);
        assert!(needle.l > meter_level(0.5) * 0.9, "up in 300ms: {}", needle.l);
        let high = needle.l;
        needle.at = Instant::now() - std::time::Duration::from_millis(50);
        meter.push(Levels::default());
        needle.step(&meter);
        assert!(needle.l < high && needle.l > high * 0.5, "falls slowly: {} from {high}", needle.l);
        assert!(needle.peak_l >= high, "peak hangs");
    }
}
