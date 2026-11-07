//! Live levels and samples for the meters. Written from the audio callback, read by the screen.
use std::sync::Mutex;
use std::time::Instant;

const RING: usize = 1024;
const SAMPLES: usize = 4096;
/// Samples per spectrum. About 21 Hz a bin at 44.1 kHz, fine enough to split the kick from the bass.
pub const FFT_N: usize = 2048;

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
    samples: Vec<(f32, f32)>,
    written: usize,
    rate: u32,
}

pub struct Meter {
    inner: Mutex<Inner>,
}

impl Default for Meter {
    fn default() -> Self {
        Self { inner: Mutex::new(Inner { now: Levels::default(), last: None, blocks: 0, ring: [0.0; RING], samples: vec![(0.0, 0.0); SAMPLES], written: 0, rate: 44100 }) }
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

    /// Raw samples for the scope and the spectrum. Skips like `push`: a lost slice is one rough frame, nothing more.
    pub fn push_samples(&self, new: &[(f32, f32)], rate: u32) {
        let Ok(mut m) = self.inner.try_lock() else { return };
        m.rate = rate;
        for s in new {
            let at = m.written % SAMPLES;
            m.samples[at] = *s;
            m.written += 1;
        }
    }

    /// Fake kick-and-floor audio for renders with no stream. `t` is seconds.
    pub fn push_demo(&self, t: f32) {
        let v = 0.08 + 0.55 * (1.0 - (t / 0.46).rem_euclid(1.0)).powi(3);
        self.push(Levels { rms_l: v, rms_r: v * 0.9, peak_l: v * 1.3, peak_r: v * 1.2 });
        let samples: Vec<(f32, f32)> = (0..2205).map(|i| demo_sample(t - 0.05 + i as f32 / 44100.0)).collect();
        self.push_samples(&samples, 44100);
    }

    /// Current levels, falling to silence when audio stops arriving.
    pub fn read(&self) -> Levels {
        let m = self.inner.lock().unwrap();
        let fade = m.last.map_or(0.0, |l| (1.0 - l.elapsed().as_secs_f32() / 0.5).max(0.0));
        Levels { rms_l: m.now.rms_l * fade, rms_r: m.now.rms_r * fade, peak_l: m.now.peak_l * fade, peak_r: m.now.peak_r * fade }
    }

    /// The newest `count` samples mixed to mono, with the sample rate.
    pub fn samples(&self, count: usize) -> (Vec<f32>, u32) {
        let (stereo, rate) = self.stereo(count);
        (stereo.iter().map(|(l, r)| (l + r) * 0.5).collect(), rate)
    }

    /// The newest `count` left and right samples with the sample rate, oldest first. Zeros when the audio has stopped.
    pub fn stereo(&self, count: usize) -> (Vec<(f32, f32)>, u32) {
        let m = self.inner.lock().unwrap();
        let count = count.min(SAMPLES);
        if !m.last.is_some_and(|l| l.elapsed().as_secs_f32() < 0.5) {
            return (vec![(0.0, 0.0); count], m.rate);
        }
        let start = m.written + SAMPLES - count;
        ((0..count).map(|i| m.samples[(start + i) % SAMPLES]).collect(), m.rate)
    }

    /// The newest `count` block levels, oldest first.
    pub fn recent(&self, count: usize) -> Vec<f32> {
        let m = self.inner.lock().unwrap();
        let live = m.last.is_some_and(|l| l.elapsed().as_secs_f32() < 0.5);
        let n = count.min(m.blocks).min(RING);
        (0..n).map(|i| if live { m.ring[(m.blocks - n + i) % RING] } else { 0.0 }).collect()
    }
}

/// A kick, a rolling bass and off-beat hats, so the demo's meters have something to chew on. The hats and a slow
/// detune give it some stereo width.
fn demo_sample(t: f32) -> (f32, f32) {
    use std::f32::consts::TAU;
    let beat = (t / 0.46).rem_euclid(1.0) * 0.46;
    let kick = (TAU * (48.0 * beat + 2.2 * (1.0 - (-beat * 30.0).exp()))).sin() * (-beat * 7.0).exp();
    let bass = (t * 55.0).rem_euclid(1.0) * 2.0 - 1.0;
    let off = ((t / 0.46 + 0.5).rem_euclid(1.0)) * 0.46;
    let hat = ((t * 41_000.0).sin() + (t * 53_711.0).sin() + (t * 67_391.0).sin()) * (-off * 45.0).exp();
    let centre = kick * 0.7 + bass * 0.12 * (1.0 - (-beat * 12.0).exp());
    let side = hat * 0.02 + (TAU * 220.0 * t).sin() * 0.12 * (TAU * 0.3 * t).sin();
    (centre + side, centre - side)
}

/// Collects samples into 50ms blocks inside the audio callback.
pub struct Blocks {
    rate: u32,
    tail: [(f32, f32); 512],
    tail_n: usize,
    size: usize,
    n: usize,
    sum_l: f32,
    sum_r: f32,
    peak_l: f32,
    peak_r: f32,
}

impl Blocks {
    pub fn new(rate: u32) -> Self {
        Self { rate, tail: [(0.0, 0.0); 512], tail_n: 0, size: (rate as usize / 20).max(1), n: 0, sum_l: 0.0, sum_r: 0.0, peak_l: 0.0, peak_r: 0.0 }
    }

    pub fn add(&mut self, l: f32, r: f32, meter: &Meter) {
        self.tail[self.tail_n] = (l, r);
        self.tail_n += 1;
        if self.tail_n == self.tail.len() {
            meter.push_samples(&self.tail, self.rate);
            self.tail_n = 0;
        }
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

/// In-place radix-2 FFT. `re` and `im` must be the same power-of-two length.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut size = 2;
    while size <= n {
        let turn = -std::f32::consts::TAU / size as f32;
        for start in (0..n).step_by(size) {
            for k in 0..size / 2 {
                let (s, c) = (turn * k as f32).sin_cos();
                let (a, b) = (start + k, start + k + size / 2);
                let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                (re[b], im[b]) = (re[a] - tr, im[a] - ti);
                re[a] += tr;
                im[a] += ti;
            }
        }
        size <<= 1;
    }
}

/// `n` log-spaced bands from 40 Hz to 16 kHz, each 0..=1. Tilted up 3 dB an octave, or the highs of real music
/// would never lift off the floor next to the kick.
pub fn bands(samples: &[f32], rate: u32, n: usize) -> Vec<f32> {
    let size = samples.len();
    if size == 0 || !size.is_power_of_two() || samples.iter().all(|s| *s == 0.0) {
        return vec![0.0; n];
    }
    let hann = |i: usize| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / size as f32).cos();
    let mut re: Vec<f32> = samples.iter().enumerate().map(|(i, s)| s * hann(i)).collect();
    let mut im = vec![0.0; size];
    fft(&mut re, &mut im);
    let hz_per_bin = rate as f32 / size as f32;
    // A full-scale sine lands in its bin at size / 4 through the Hann window.
    let full = (size / 4) as f32;
    (0..n)
        .map(|b| {
            let edge = |b: usize| 40.0 * 400f32.powf(b as f32 / n as f32);
            let lo = (edge(b) / hz_per_bin).round() as usize;
            let hi = ((edge(b + 1) / hz_per_bin).round() as usize).clamp(lo + 1, size / 2);
            let power: f32 = (lo..hi).map(|i| re[i] * re[i] + im[i] * im[i]).sum::<f32>() / (full * full);
            let tilt = 3.0 * (edge(b) / 40.0).log2();
            ((10.0 * power.max(1e-9).log10() + tilt + 48.0) / 48.0).clamp(0.0, 1.0)
        })
        .collect()
}

/// Bars that jump up and fall back, with a cap that floats down after them.
pub struct Spectrum {
    pub bars: Vec<f32>,
    pub caps: Vec<f32>,
    at: Instant,
}

impl Default for Spectrum {
    fn default() -> Self {
        Self { bars: vec![], caps: vec![], at: Instant::now() }
    }
}

impl Spectrum {
    pub fn step(&mut self, levels: &[f32]) {
        let now = Instant::now();
        let dt = now.duration_since(self.at).as_secs_f32();
        self.at = now;
        self.bars.resize(levels.len(), 0.0);
        self.caps.resize(levels.len(), 0.0);
        for (i, v) in levels.iter().enumerate() {
            self.bars[i] = v.max(self.bars[i] - dt * 1.8);
            self.caps[i] = self.bars[i].max(self.caps[i] - dt * 0.35);
        }
    }
}

/// `want` samples starting at a rising zero crossing, so the trace stands still like a triggered scope. Crossings
/// come from a smoothed copy, or hats would make it jump around.
pub fn triggered(samples: &[f32], want: usize) -> &[f32] {
    let search = samples.len().saturating_sub(want);
    let mut smooth = 0.0;
    let mut at = search;
    for (i, s) in samples.iter().take(search).enumerate() {
        let next = smooth + (s - smooth) * 0.05;
        if i > 64 && smooth <= 0.0 && next > 0.0 {
            at = i;
            break;
        }
        smooth = next;
    }
    &samples[at..(at + want).min(samples.len())]
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
    fn blocks_keep_the_samples_as_a_mono_mix() {
        let (meter, mut blocks) = (Meter::default(), Blocks::new(48000));
        for i in 0..2400 {
            blocks.add(i as f32, i as f32 + 2.0, &meter);
        }
        let (s, rate) = meter.samples(3);
        assert_eq!(rate, 48000);
        assert_eq!(meter.stereo(1).0, [(2047.0, 2049.0)], "left and right stay apart");
        assert_eq!(s, [2046.0, 2047.0, 2048.0], "newest whole slice, oldest first, mixed");
    }

    #[test]
    fn stopped_audio_gives_silent_samples() {
        assert!(Meter::default().samples(8).0.iter().all(|s| *s == 0.0));
        assert_eq!(bands(&[0.0; 2048], 44100, 8), [0.0; 8]);
    }

    fn sine(hz: f32, n: usize, phase: f32) -> Vec<f32> {
        (0..n).map(|i| (std::f32::consts::TAU * hz * i as f32 / 44100.0 + phase).sin()).collect()
    }

    #[test]
    fn a_tone_lights_its_own_band() {
        for hz in [60.0, 1000.0, 9000.0] {
            let b = bands(&sine(hz, FFT_N, 0.0), 44100, 20);
            let loudest = b.iter().enumerate().max_by(|x, y| x.1.total_cmp(y.1)).unwrap().0;
            let expect = (20.0 * (hz / 40.0).ln() / 400f32.ln()) as usize;
            assert!(loudest.abs_diff(expect) <= 1, "{hz} Hz lit band {loudest}, wanted {expect}: {b:?}");
            assert!(b[loudest] > 0.9, "a full-scale tone reaches the top: {b:?}");
            assert!(b[(expect + 10) % 20] < b[loudest] * 0.5, "far bands stay low: {b:?}");
        }
    }

    #[test]
    fn spectrum_jumps_up_and_the_cap_floats_down() {
        let mut s = Spectrum::default();
        s.step(&[1.0, 0.0]);
        assert_eq!(s.bars[0], 1.0);
        s.at = Instant::now() - std::time::Duration::from_millis(200);
        s.step(&[0.0, 0.0]);
        assert!(s.bars[0] < 0.7 && s.bars[0] > 0.5, "falls: {}", s.bars[0]);
        assert!(s.caps[0] > 0.9, "cap hangs on: {}", s.caps[0]);
    }

    #[test]
    fn the_trigger_holds_the_trace_still_whatever_the_phase() {
        let first: Vec<f32> = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0]
            .iter()
            .map(|phase| {
                let s = sine(440.0, 3072, *phase);
                triggered(&s, 1024)[100]
            })
            .collect();
        for v in &first {
            assert!((v - first[0]).abs() < 0.1, "{first:?}");
        }
        assert_eq!(triggered(&[0.0; 8], 16).len(), 8, "short input just comes back");
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
