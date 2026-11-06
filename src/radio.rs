//! Plays a DI stream through exactly one socket per play, because DI allows one stream per key and the newest wins.
//! Hopping waits for the old socket to close before opening the new one, so we never kick ourselves.
use crate::meter::{Blocks, Meter};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer, RingBuffer};
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Connecting,
    Buffering,
    Playing,
    Reconnecting,
    Refused,
    NoAudio,
}

#[derive(Clone)]
pub struct Timing {
    /// Waits after a network failure; the last one repeats forever.
    pub retry: Vec<Duration>,
    /// Waits after a 401 or 403, then give up.
    pub refused: Vec<Duration>,
    /// A cut this soon is likely a kick. Reconnect once in case it was a blip; a second one means another device
    /// wants the key, and fighting it would kill both.
    pub kick_window: Duration,
    pub kick_limit: u32,
    pub preroll: f32,
    /// No bytes for this long means the stream is dead.
    pub stall: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        let secs = |v: &[f32]| v.iter().map(|s| Duration::from_secs_f32(*s)).collect();
        Self {
            retry: secs(&[0.5, 1.0, 2.0, 4.0, 8.0, 15.0, 30.0]),
            refused: secs(&[1.5, 5.0, 15.0, 30.0]),
            kick_window: Duration::from_secs(30),
            kick_limit: 2,
            preroll: 1.0,
            stall: Duration::from_secs(10),
        }
    }
}

/// Where the sound goes. `Silent` runs at real speed with no device, so tests make no noise.
#[derive(Clone, Copy, PartialEq)]
pub enum Sink {
    Device,
    Silent,
}

#[derive(Default)]
struct Cancel {
    fired: AtomicBool,
    socket: Mutex<Option<TcpStream>>,
}

impl Cancel {
    fn fire(&self) {
        self.fired.store(true, Relaxed);
        if let Some(s) = self.socket.lock().unwrap().take() {
            let _ = s.shutdown(Shutdown::Both);
        }
    }
    fn fired(&self) -> bool {
        self.fired.load(Relaxed)
    }
}

struct Shared {
    state: Mutex<State>,
    frames: AtomicU64,
    rate: AtomicU32,
}

pub struct Radio {
    pub meter: Arc<Meter>,
    pub timing: Timing,
    pub sink: Sink,
    shared: Arc<Shared>,
    job: Option<(Arc<Cancel>, JoinHandle<()>)>,
    closing: Option<JoinHandle<()>>,
}

impl Radio {
    pub fn new(sink: Sink) -> Self {
        Self {
            meter: Arc::default(),
            timing: Timing::default(),
            sink,
            shared: Arc::new(Shared { state: Mutex::new(State::Idle), frames: AtomicU64::new(0), rate: AtomicU32::new(44100) }),
            job: None,
            closing: None,
        }
    }

    pub fn state(&self) -> State {
        *self.shared.state.lock().unwrap()
    }

    /// Seconds of audio rendered since the last play.
    pub fn rendered(&self) -> f64 {
        self.shared.frames.load(Relaxed) as f64 / self.shared.rate.load(Relaxed) as f64
    }

    pub fn play(&mut self, urls: Vec<String>) {
        self.halt();
        *self.shared.state.lock().unwrap() = State::Connecting;
        let cancel = Arc::new(Cancel::default());
        let previous = self.closing.take();
        let ctx = Ctx { cancel: cancel.clone(), shared: self.shared.clone(), meter: self.meter.clone(), timing: self.timing.clone(), sink: self.sink, flow: Arc::default() };
        let handle = thread::spawn(move || {
            if let Some(p) = previous {
                let _ = p.join();
            }
            ctx.shared.frames.store(0, Relaxed);
            run(urls, &ctx);
        });
        self.job = Some((cancel, handle));
    }

    pub fn stop(&mut self) {
        self.halt();
        *self.shared.state.lock().unwrap() = State::Idle;
    }

    /// Cuts the socket at once and leaves the join to the next play, so the UI never waits on the network.
    fn halt(&mut self) {
        if let Some((cancel, handle)) = self.job.take() {
            cancel.fire();
            self.closing = Some(handle);
        }
    }
}

impl Drop for Radio {
    fn drop(&mut self) {
        self.halt();
    }
}

#[derive(Default)]
struct Flow {
    started: AtomicBool,
    lost: AtomicBool,
}

struct Ctx {
    cancel: Arc<Cancel>,
    shared: Arc<Shared>,
    meter: Arc<Meter>,
    timing: Timing,
    sink: Sink,
    flow: Arc<Flow>,
}

impl Ctx {
    fn set(&self, s: State) {
        // Under the lock, so a worker that was cancelled can never overwrite the state of its replacement.
        let mut state = self.shared.state.lock().unwrap();
        if !self.cancel.fired() {
            *state = s;
        }
    }
    fn state(&self) -> State {
        *self.shared.state.lock().unwrap()
    }
    /// False if cancelled during the wait.
    fn wait(&self, d: Duration) -> bool {
        let end = Instant::now() + d;
        while Instant::now() < end {
            if self.cancel.fired() {
                return false;
            }
            thread::sleep(Duration::from_millis(20).min(end - Instant::now()));
        }
        !self.cancel.fired()
    }
}

enum End {
    Refused,
    /// Could not connect, or the server answered with an error.
    Failed,
    /// Was connected and streaming, then it ended.
    Cut { lasted: Duration, got_audio: bool },
    OutputLost,
    NoDevice,
    Cancelled,
}

fn run(urls: Vec<String>, ctx: &Ctx) {
    let t = &ctx.timing;
    let (mut server, mut attempts, mut refusals, mut kicks) = (0usize, 0usize, 0usize, 0u32);
    let mut output: Option<Output> = None;
    loop {
        let retry = |attempts: &mut usize| {
            if ctx.state() != State::Refused {
                ctx.set(State::Reconnecting);
            }
            let d = t.retry[(*attempts).min(t.retry.len() - 1)];
            *attempts += 1;
            d
        };
        let delay = match session(&urls[server % urls.len()], ctx, &mut output, &mut attempts, &mut refusals) {
            End::Cancelled => return,
            End::NoDevice => return ctx.set(State::NoAudio),
            End::OutputLost => {
                // AirPods in or out. The buffered audio went with the old output, so start over straight away.
                output = None;
                ctx.set(State::Connecting);
                continue;
            }
            End::Refused => {
                ctx.set(if refusals >= 1 { State::Refused } else { State::Reconnecting });
                let Some(d) = t.refused.get(refusals) else { return ctx.set(State::Refused) };
                refusals += 1;
                *d
            }
            End::Failed => {
                server += 1;
                retry(&mut attempts)
            }
            End::Cut { lasted, got_audio } => {
                if got_audio && lasted < t.kick_window {
                    kicks += 1;
                    if kicks >= t.kick_limit {
                        return ctx.set(State::Refused);
                    }
                } else {
                    kicks = 0;
                }
                retry(&mut attempts)
            }
        };
        if !ctx.wait(delay) {
            return;
        }
    }
}

/// shortcut: plain http only, because DI's playlists only list http://premN.di.fm URLs. Add TLS if that changes.
fn split_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("http://")?;
    Some(match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    })
}

/// Sends the request and reads the headers. Returns the status and any body bytes that came along. Never logs the
/// URL: it carries the listen key.
fn open(url: &str, ctx: &Ctx) -> io::Result<(TcpStream, u16, Vec<u8>)> {
    let (host, path) = split_url(url).ok_or_else(|| io::Error::other("not an http url"))?;
    let authority = if host.contains(':') { host.to_string() } else { format!("{host}:80") };
    let mut last = io::Error::other("no address");
    let mut stream = None;
    for addr in authority.to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, ctx.timing.stall) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => last = e,
        }
    }
    let mut stream = stream.ok_or(last)?;
    stream.set_read_timeout(Some(Duration::from_millis(250)))?;
    *ctx.cancel.socket.lock().unwrap() = Some(stream.try_clone()?);
    if ctx.cancel.fired() {
        return Err(io::Error::other("cancelled"));
    }
    write!(stream, "GET {path} HTTP/1.0\r\nHost: {host}\r\nUser-Agent: BLIKSEM\r\nConnection: close\r\n\r\n")?;

    let (mut buf, mut chunk, start) = (Vec::new(), [0u8; 4096], Instant::now());
    let end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        // Server input: a real header block is small, so don't buffer forever.
        if buf.len() > 64 * 1024 || start.elapsed() > ctx.timing.stall || ctx.cancel.fired() {
            return Err(io::Error::other("bad response"));
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted) => {}
            Err(e) => return Err(e),
        }
    };
    // "HTTP/1.0 200 OK" or Icecast's old "ICY 200 OK".
    let status = String::from_utf8_lossy(&buf[..end]).split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let rest = buf.split_off(end);
    Ok((stream, status, rest))
}

/// The response body as a plain reader for the decoder, watching cancel, stalls and the output while it waits.
struct Body {
    stream: TcpStream,
    early: Vec<u8>,
    cancel: Arc<Cancel>,
    shared: Arc<Shared>,
    flow: Arc<Flow>,
    stall: Duration,
    device: Option<String>,
    checked: Instant,
}

impl Read for Body {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.early.is_empty() {
            let n = buf.len().min(self.early.len());
            buf[..n].copy_from_slice(&self.early[..n]);
            self.early.drain(..n);
            return Ok(n);
        }
        let quiet = Instant::now();
        loop {
            if self.cancel.fired() || self.flow.lost.load(Relaxed) {
                return Err(io::Error::other("stopped"));
            }
            if !self.flow.started.load(Relaxed) {
                // Ran dry with the connection still up: show that, without letting a cancelled worker write.
                let mut state = self.shared.state.lock().unwrap();
                if *state == State::Playing && !self.cancel.fired() {
                    *state = State::Buffering;
                }
            }
            // The system output moved (AirPods). Linux follows by itself through PipeWire; this catches the Mac.
            if self.device.is_some() && self.checked.elapsed() > Duration::from_secs(2) {
                self.checked = Instant::now();
                if default_device_id() != self.device {
                    self.flow.lost.store(true, Relaxed);
                }
            }
            match self.stream.read(buf) {
                Ok(n) => return Ok(n),
                Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted) => {
                    if quiet.elapsed() > self.stall {
                        return Err(io::ErrorKind::TimedOut.into());
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }
}

fn session(url: &str, ctx: &Ctx, output: &mut Option<Output>, attempts: &mut usize, refusals: &mut usize) -> End {
    if ctx.cancel.fired() {
        return End::Cancelled;
    }
    let (stream, status, early) = match open(url, ctx) {
        Ok(v) => v,
        Err(_) if ctx.cancel.fired() => return End::Cancelled,
        Err(_) => return End::Failed,
    };
    match status {
        200 => {}
        401 | 403 => return End::Refused,
        _ => return End::Failed,
    }
    let connected = Instant::now();
    *attempts = 0;
    let body = Body {
        stream,
        early,
        cancel: ctx.cancel.clone(),
        shared: ctx.shared.clone(),
        flow: ctx.flow.clone(),
        stall: ctx.timing.stall,
        device: output.as_ref().and_then(|o| o.device_id.clone()),
        checked: Instant::now(),
    };
    let got_audio = match decode(body, ctx, output, refusals) {
        Ok(got) => got,
        Err(end) => return end,
    };
    if ctx.cancel.fired() {
        return End::Cancelled;
    }
    if ctx.flow.lost.load(Relaxed) {
        return End::OutputLost;
    }
    End::Cut { lasted: connected.elapsed(), got_audio }
}

/// Decodes until the stream ends. Returns whether any audio came out.
fn decode(body: Body, ctx: &Ctx, output: &mut Option<Output>, refusals: &mut usize) -> Result<bool, End> {
    let source = MediaSourceStream::new(Box::new(ReadOnlySource::new(body)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3"); // premium_high is always MP3
    let Ok(mut format) = symphonia::default::get_probe().probe(&hint, source, FormatOptions::default(), MetadataOptions::default()) else { return Ok(false) };
    let Some(track) = format.default_track(TrackType::Audio) else { return Ok(false) };
    let track_id = track.id;
    let Some(params) = track.codec_params.as_ref().and_then(|p| p.audio()) else { return Ok(false) };
    let Ok(mut decoder) = symphonia::default::get_codecs().make_audio_decoder(params, &AudioDecoderOptions::default()) else { return Ok(false) };

    let (mut got_audio, mut interleaved, mut stereo) = (false, Vec::<f32>::new(), Vec::<f32>::new());
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            _ => return Ok(got_audio),
        };
        if packet.track_id != track_id {
            continue;
        }
        let audio = match decoder.decode(&packet) {
            Ok(a) => a,
            Err(DecodeError::DecodeError(_)) => continue, // one bad frame on a live stream is not worth a reconnect
            Err(_) => return Ok(got_audio),
        };
        let rate = audio.spec().rate();
        let channels = audio.spec().channels().count().max(1);
        interleaved.resize(audio.samples_interleaved(), 0.0);
        audio.copy_to_slice_interleaved(&mut interleaved);
        stereo.clear();
        for frame in interleaved.chunks_exact(channels) {
            stereo.push(frame[0]);
            stereo.push(*frame.get(1).unwrap_or(&frame[0]));
        }
        if output.as_ref().is_none_or(|o| o.source_rate != rate) {
            *output = None;
            *output = Some(Output::new(ctx, rate).ok_or(End::NoDevice)?);
        }
        let out = output.as_mut().unwrap();
        if !out.push(&stereo, &ctx.cancel) {
            return Err(End::Cancelled);
        }
        got_audio = true;
        *refusals = 0;
        if !ctx.flow.started.load(Relaxed) && out.queued_secs() >= ctx.timing.preroll {
            ctx.flow.started.store(true, Relaxed);
        }
        if ctx.flow.started.load(Relaxed) {
            ctx.set(State::Playing);
        } else if ctx.state() != State::Refused {
            ctx.set(State::Buffering);
        }
    }
}

fn default_device_id() -> Option<String> {
    cpal::default_host().default_output_device()?.id().ok().map(|id| id.to_string())
}

/// Runs inside the audio callback: no locks that can block, no allocation.
struct Render {
    consumer: Consumer<f32>,
    flow: Arc<Flow>,
    shared: Arc<Shared>,
    meter: Arc<Meter>,
    blocks: Blocks,
}

impl Render {
    fn fill(&mut self, out: &mut [f32], channels: usize) {
        for frame in out.chunks_mut(channels) {
            let mut sample = (0.0, 0.0);
            if self.flow.started.load(Relaxed) {
                if self.consumer.slots() >= 2 {
                    sample = (self.consumer.pop().unwrap_or(0.0), self.consumer.pop().unwrap_or(0.0));
                    self.blocks.add(sample.0, sample.1, &self.meter);
                    self.shared.frames.fetch_add(1, Relaxed);
                } else {
                    // Ran dry: hold until there is enough to play smoothly again.
                    self.flow.started.store(false, Relaxed);
                }
            }
            match frame {
                [mono] => *mono = (sample.0 + sample.1) * 0.5,
                [l, r, rest @ ..] => {
                    (*l, *r) = sample;
                    rest.fill(0.0);
                }
                [] => {}
            }
        }
    }
}

enum Target {
    Device(cpal::Stream),
    Silent(Arc<AtomicBool>, Option<JoinHandle<()>>),
}

struct Output {
    producer: Producer<f32>,
    source_rate: u32,
    rate: u32,
    phase: f64,
    last: (f32, f32),
    device_id: Option<String>,
    _target: Target,
}

impl Drop for Target {
    fn drop(&mut self) {
        if let Target::Silent(stop, handle) = self {
            stop.store(true, Relaxed);
            if let Some(h) = handle.take() {
                let _ = h.join();
            }
        }
    }
}

impl Output {
    fn new(ctx: &Ctx, source_rate: u32) -> Option<Self> {
        ctx.flow.started.store(false, Relaxed);
        ctx.flow.lost.store(false, Relaxed);
        let open_device = || -> Option<(cpal::Device, cpal::StreamConfig)> {
            let device = cpal::default_host().default_output_device()?;
            // Prefer the stream's own rate so nothing gets resampled.
            let exact = device.supported_output_configs().ok()?.find(|c| c.sample_format() == cpal::SampleFormat::F32 && c.channels() == 2 && c.min_sample_rate() <= source_rate && source_rate <= c.max_sample_rate());
            let config: cpal::StreamConfig = match exact {
                Some(c) => c.with_sample_rate(source_rate).config(),
                None => device.default_output_config().ok()?.config(),
            };
            Some((device, config))
        };
        let device = if ctx.sink == Sink::Device { Some(open_device()?) } else { None };
        let rate = device.as_ref().map_or(source_rate, |(_, c)| c.sample_rate);
        ctx.shared.rate.store(rate, Relaxed);
        let (producer, consumer) = RingBuffer::new(rate as usize * 2 * 6);
        let mut render = Render { consumer, flow: ctx.flow.clone(), shared: ctx.shared.clone(), meter: ctx.meter.clone(), blocks: Blocks::new(rate) };
        let (target, device_id) = match device {
            Some((device, config)) => {
                let channels = config.channels as usize;
                let flow = ctx.flow.clone();
                let id = device.id().ok().map(|i| i.to_string());
                let stream = device
                    .build_output_stream(config, move |data: &mut [f32], _: &cpal::OutputCallbackInfo| render.fill(data, channels), move |_| flow.lost.store(true, Relaxed), None)
                    .ok()?;
                stream.play().ok()?;
                (Target::Device(stream), id)
            }
            None => {
                let stop = Arc::new(AtomicBool::new(false));
                let flag = stop.clone();
                let handle = thread::spawn(move || {
                    let (mut last, mut buf) = (Instant::now(), vec![0f32; 0]);
                    while !flag.load(Relaxed) {
                        thread::sleep(Duration::from_millis(5));
                        let frames = (last.elapsed().as_secs_f64() * rate as f64) as usize;
                        last += Duration::from_secs_f64(frames as f64 / rate as f64);
                        buf.resize(frames * 2, 0.0);
                        render.fill(&mut buf, 2);
                    }
                });
                (Target::Silent(stop, Some(handle)), None)
            }
        };
        Some(Self { producer, source_rate, rate, phase: 0.0, last: (0.0, 0.0), device_id, _target: target })
    }

    fn queued_secs(&self) -> f32 {
        let used = self.producer.buffer().capacity() - self.producer.slots();
        used as f32 / 2.0 / self.rate as f32
    }

    /// Queue stereo audio, waiting while the ring is full. False if cancelled.
    fn push(&mut self, stereo: &[f32], cancel: &Cancel) -> bool {
        // shortcut: linear resampling, only when the device refuses the stream's rate. Add a real resampler if that ever shows up in practice.
        let step = self.source_rate as f64 / self.rate as f64;
        for frame in stereo.chunks_exact(2) {
            let (l, r) = (frame[0], frame[1]);
            while self.phase < 1.0 {
                let p = self.phase as f32;
                let out = (self.last.0 + (l - self.last.0) * p, self.last.1 + (r - self.last.1) * p);
                while self.producer.slots() < 2 {
                    if cancel.fired() {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                let _ = self.producer.push(out.0);
                let _ = self.producer.push(out.1);
                self.phase += step;
            }
            self.phase -= 1.0;
            self.last = (l, r);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::atomic::AtomicUsize;

    const TONE: &[u8] = include_bytes!("../assets/test-tone.mp3");

    /// A local stand-in for DI's Icecast, so playback tests never touch a real key or stream.
    struct Fake {
        url: String,
        connections: Arc<AtomicUsize>,
        most_at_once: Arc<AtomicUsize>,
        stop: Arc<AtomicBool>,
    }

    #[derive(Clone, Copy)]
    struct Rules {
        refuse_until: usize,
        cut_after: Option<f32>,
        stall_after: Option<f32>,
    }

    const SERVE: Rules = Rules { refuse_until: 0, cut_after: None, stall_after: None };

    impl Fake {
        fn start(rules: Rules) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/techno_hi?listen_key=fake", listener.local_addr().unwrap());
            let (connections, most, active, stop) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)), Arc::new(AtomicBool::new(false)));
            let (c, m, halt) = (connections.clone(), most.clone(), stop.clone());
            listener.set_nonblocking(true).unwrap();
            thread::spawn(move || {
                while !halt.load(Relaxed) {
                    let Ok((sock, _)) = listener.accept() else {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    };
                    sock.set_nonblocking(false).unwrap();
                    let n = c.fetch_add(1, Relaxed) + 1;
                    let (active, most, halt) = (active.clone(), m.clone(), halt.clone());
                    thread::spawn(move || serve(sock, n, rules, active, most, halt));
                }
            });
            Self { url, connections, most_at_once: most, stop }
        }
        fn connections(&self) -> usize {
            self.connections.load(Relaxed)
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.stop.store(true, Relaxed);
        }
    }

    fn serve(mut sock: TcpStream, n: usize, rules: Rules, active: Arc<AtomicUsize>, most: Arc<AtomicUsize>, halt: Arc<AtomicBool>) {
        let mut req = [0u8; 2048];
        let _ = sock.read(&mut req);
        if n <= rules.refuse_until {
            let _ = sock.write_all(b"HTTP/1.0 401 Authentication Required\r\nContent-Length: 0\r\n\r\n");
            return;
        }
        most.fetch_max(active.fetch_add(1, Relaxed) + 1, Relaxed);
        // A blocked read returns the moment the client hangs up, so the count of open sockets is exact.
        let (mut watch, gone) = (sock.try_clone().unwrap(), Arc::new(AtomicBool::new(false)));
        let (flag, count) = (gone.clone(), active.clone());
        thread::spawn(move || {
            let mut b = [0u8; 64];
            while !matches!(watch.read(&mut b), Ok(0) | Err(_)) {}
            flag.store(true, Relaxed);
            count.fetch_sub(1, Relaxed);
        });
        let _ = sock.write_all(b"HTTP/1.0 200 OK\r\nContent-Type: audio/mpeg\r\n\r\n");
        // Icecast's 2s burst, then real time at the file's 128 kbps.
        let (mut sent, start) = (0usize, Instant::now());
        while !gone.load(Relaxed) && !halt.load(Relaxed) {
            let t = start.elapsed().as_secs_f32();
            if rules.cut_after.is_some_and(|c| t > c) {
                let _ = sock.shutdown(Shutdown::Both);
                return;
            }
            if !rules.stall_after.is_some_and(|s| t > s) {
                let want = 32000 + (t * 16000.0) as usize;
                while sent < want {
                    let at = sent % TONE.len();
                    let piece = &TONE[at..TONE.len().min(at + want - sent)];
                    if sock.write_all(piece).is_err() {
                        return;
                    }
                    sent += piece.len();
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn radio() -> Radio {
        let mut r = Radio::new(Sink::Silent);
        r.timing.preroll = 0.2;
        r.timing.retry = vec![Duration::from_millis(50)];
        r.timing.refused = vec![Duration::from_millis(50), Duration::from_millis(100)];
        r
    }

    fn until(what: &str, secs: f32, mut f: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs_f32(secs);
        while Instant::now() < end {
            if f() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for {what}");
    }

    #[test]
    fn plays_past_the_old_ten_second_drop_on_one_connection() {
        let server = Fake::start(SERVE);
        let mut r = radio();
        r.play(vec![server.url.clone()]);
        until("playing", 3.0, || r.state() == State::Playing);
        thread::sleep(Duration::from_secs(12));
        assert_eq!(r.state(), State::Playing);
        assert!(r.rendered() > 11.0, "rendered {}", r.rendered());
        assert_eq!(server.connections(), 1, "one socket per play");
    }

    #[test]
    fn decoded_audio_moves_the_meter() {
        let server = Fake::start(SERVE);
        let mut r = radio();
        r.play(vec![server.url.clone()]);
        until("playing", 3.0, || r.state() == State::Playing);
        thread::sleep(Duration::from_millis(300));
        let v = r.meter.read();
        // The fixture is a sine peaking at 1/16 on each channel, so RMS is about 0.044 and peak/RMS is the square root of 2.
        assert!((0.03..0.05).contains(&v.rms_l) && (0.03..0.05).contains(&v.rms_r), "{v:?}");
        assert!((v.peak_l / v.rms_l - std::f32::consts::SQRT_2).abs() < 0.05, "not a clean sine: {v:?}");
        assert!(!r.meter.recent(5).is_empty());
        r.stop();
        until("the meter to fall", 1.0, || r.meter.read().rms_l == 0.0);
    }

    #[test]
    fn reconnects_after_a_cut_and_goes_back_to_playing() {
        let server = Fake::start(Rules { cut_after: Some(0.8), ..SERVE });
        let mut r = radio();
        r.timing.kick_window = Duration::ZERO; // not testing kicks here
        r.play(vec![server.url.clone()]);
        until("a second connection", 4.0, || server.connections() >= 2);
        until("playing again", 3.0, || r.state() == State::Playing);
    }

    #[test]
    fn stops_fighting_when_kicked_twice() {
        let server = Fake::start(Rules { cut_after: Some(0.5), ..SERVE });
        let mut r = radio();
        r.play(vec![server.url.clone()]);
        until("refused", 6.0, || r.state() == State::Refused);
        thread::sleep(Duration::from_millis(400));
        assert_eq!(server.connections(), 2, "gave up instead of fighting the other device");
    }

    #[test]
    fn a_refused_key_gives_up_after_the_bounded_retries() {
        let server = Fake::start(Rules { refuse_until: usize::MAX, ..SERVE });
        let mut r = radio();
        r.play(vec![server.url.clone()]);
        until("refused", 3.0, || r.state() == State::Refused && server.connections() >= 3);
        thread::sleep(Duration::from_millis(400));
        assert_eq!(server.connections(), 3, "first try plus the two waits");
        assert_eq!(r.state(), State::Refused);
    }

    #[test]
    fn recovers_when_the_refusal_clears() {
        let server = Fake::start(Rules { refuse_until: 2, ..SERVE });
        let mut r = radio();
        r.play(vec![server.url.clone()]);
        until("playing", 4.0, || r.state() == State::Playing);
        assert_eq!(server.connections(), 3);
    }

    #[test]
    fn falls_back_to_the_next_server() {
        // Bind then drop, so this port is closed.
        let dead = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        let server = Fake::start(SERVE);
        let mut r = radio();
        r.play(vec![format!("http://{dead}/x"), server.url.clone()]);
        until("playing", 4.0, || r.state() == State::Playing);
    }

    #[test]
    fn a_stalled_stream_counts_as_dead() {
        let server = Fake::start(Rules { stall_after: Some(0.3), ..SERVE });
        let mut r = radio();
        r.timing.stall = Duration::from_millis(500);
        r.timing.kick_window = Duration::ZERO;
        r.play(vec![server.url.clone()]);
        until("a reconnect", 6.0, || server.connections() >= 2);
    }

    #[test]
    fn hopping_never_overlaps_connections() {
        let (a, b) = (Fake::start(SERVE), Fake::start(SERVE));
        let mut r = radio();
        for i in 0..6 {
            r.play(vec![if i % 2 == 0 { a.url.clone() } else { b.url.clone() }]);
            thread::sleep(Duration::from_millis(150));
        }
        until("playing", 3.0, || r.state() == State::Playing);
        assert_eq!(a.most_at_once.load(Relaxed), 1);
        assert_eq!(b.most_at_once.load(Relaxed), 1);
        // Same server over and over is the real case for a kick.
        let c = Fake::start(SERVE);
        for _ in 0..5 {
            r.play(vec![c.url.clone()]);
            thread::sleep(Duration::from_millis(80));
        }
        assert_eq!(c.most_at_once.load(Relaxed), 1);
    }

    #[test]
    fn stop_closes_the_socket_and_goes_idle() {
        let server = Fake::start(SERVE);
        let mut r = radio();
        r.play(vec![server.url.clone()]);
        until("playing", 3.0, || r.state() == State::Playing);
        r.stop();
        assert_eq!(r.state(), State::Idle);
        thread::sleep(Duration::from_millis(200));
        let at = r.rendered();
        thread::sleep(Duration::from_millis(200));
        assert_eq!(r.rendered(), at, "nothing plays after stop");
        assert_eq!(r.state(), State::Idle, "the old worker must not overwrite the state");
        assert_eq!(server.connections(), 1);
    }

    #[test]
    fn urls_split_and_https_is_rejected() {
        assert_eq!(split_url("http://prem1.di.fm:80/techno_hi?x=1"), Some(("prem1.di.fm:80", "/techno_hi?x=1")));
        assert_eq!(split_url("http://host"), Some(("host", "/")));
        assert_eq!(split_url("https://host/x"), None);
    }
}
