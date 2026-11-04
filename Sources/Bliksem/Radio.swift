import AVFoundation
import Accelerate
import AudioToolbox

enum RadioState: Equatable { case idle, connecting, buffering, playing, reconnecting, refused }

/// Live levels from the audio tap. Written on the audio thread, read by the meters.
final class Meter {
    private let lock = NSLock()
    private var peak: (l: Float, r: Float) = (0, 0)
    private var rms: (l: Float, r: Float) = (0, 0)
    private var last = Date.distantPast
    private(set) var blocks = 0
    private var ring = [Float](repeating: 0, count: 512)

    /// Split into 50ms blocks: the tap delivers big buffers, the rolling waveform wants fine ones.
    func feed(_ b: AVAudioPCMBuffer) {
        guard let ch = b.floatChannelData, b.frameLength > 0, b.format.channelCount >= 2 else { return }
        let block = max(Int(b.format.sampleRate / 20), 1), n = Int(b.frameLength)
        lock.lock(); defer { lock.unlock() }
        for start in stride(from: 0, to: n, by: block) {
            let len = vDSP_Length(min(block, n - start))
            var lr: Float = 0, rr: Float = 0, lp: Float = 0, rp: Float = 0
            vDSP_rmsqv(ch[0] + start, 1, &lr, len); vDSP_rmsqv(ch[1] + start, 1, &rr, len)
            vDSP_maxmgv(ch[0] + start, 1, &lp, len); vDSP_maxmgv(ch[1] + start, 1, &rp, len)
            rms = (lr, rr); peak = (lp, rp); last = .now
            ring[blocks % ring.count] = max(lr, rr)
            blocks += 1
        }
    }

    /// Fake kick-and-floor audio for headless renders and CPU checks.
    func feedDemo() {
        let f = AVAudioFormat(standardFormatWithSampleRate: 44100, channels: 2)!
        guard let b = AVAudioPCMBuffer(pcmFormat: f, frameCapacity: 2205) else { return }
        b.frameLength = 2205
        let beat = Date.now.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 0.46) / 0.46
        let v = Float(0.08 + 0.55 * pow(1 - beat, 3))
        for i in 0..<2205 { b.floatChannelData![0][i] = v; b.floatChannelData![1][i] = v * (i % 2 == 0 ? 0.9 : -0.9) }
        feed(b)
    }

    /// Current levels in linear 0...1; fall to silence when audio stops arriving.
    func read() -> (rmsL: Float, rmsR: Float, peakL: Float, peakR: Float) {
        lock.lock(); defer { lock.unlock() }
        let fade = Float(max(0, 1 - Date.now.timeIntervalSince(last) / 0.5))
        return (rms.l * fade, rms.r * fade, peak.l * fade, peak.r * fade)
    }

    /// The newest `count` block levels, oldest first.
    func recent(_ count: Int) -> [Float] {
        lock.lock(); defer { lock.unlock() }
        let fade: Float = Date.now.timeIntervalSince(last) < 0.5 ? 1 : 0
        let n = min(count, min(blocks, ring.count))
        return (0..<n).map { ring[(blocks - n + $0) % ring.count] * fade }
    }
}

/// Plays an Icecast stream through its own socket: exactly one connection per play, because DI
/// allows one stream per key. AVPlayer opens two per play and never reconnects when cut.
final class Radio: NSObject, URLSessionDataDelegate {
    var onState: (RadioState) -> Void = { _ in }
    let meter = Meter()
    /// Waits between attempts: after a network failure (the last one repeats), and after a 401 (then give up).
    var retryDelays: [TimeInterval] = [0.5, 1, 2, 4, 8, 15, 30]
    var refusedDelays: [TimeInterval] = [1.5, 5, 15, 30]
    /// DI allows one stream per key and the newest wins, so a cut this soon is likely a kick. Reconnect once in
    /// case it was a blip; a second one means another device wants the key, and fighting it would kill both.
    var kickWindow: TimeInterval = 30
    var kickLimit = 2
    var preroll: TimeInterval = 1

    var muted = false { didSet { engine.mainMixerNode.outputVolume = muted ? 0 : 1 } }

    private let queue: OperationQueue = { let q = OperationQueue(); q.maxConcurrentOperationCount = 1; return q }()
    private lazy var session: URLSession = {
        let c = URLSessionConfiguration.ephemeral
        c.timeoutIntervalForRequest = 10 // no bytes for this long means the stream is dead
        c.requestCachePolicy = .reloadIgnoringLocalCacheData
        return URLSession(configuration: c, delegate: self, delegateQueue: queue)
    }()
    private let engine = AVAudioEngine()
    private let node = AVAudioPlayerNode()
    private let pcm = AVAudioFormat(standardFormatWithSampleRate: 44100, channels: 2)!
    private var wired = false

    private var urls: [URL] = []
    private var server = 0
    private var task: URLSessionDataTask?
    private var generation = 0
    private var state = RadioState.idle
    private var parser: AudioFileStreamID?
    private var converter: AVAudioConverter?
    private var queued: AVAudioFrameCount = 0
    private var started = false
    private var attempts = 0, refusals = 0, kicks = 0
    private var connectedAt: Date?
    private var gotAudio = false
    private var retry: DispatchWorkItem?
    private var engineObserver: NSObjectProtocol?

    /// Seconds of audio the node has actually rendered since the last play.
    var rendered: TimeInterval {
        guard let t = node.lastRenderTime, let p = node.playerTime(forNodeTime: t) else { return 0 }
        return Double(p.sampleTime) / p.sampleRate
    }

    func play(_ urls: [URL]) {
        queue.addOperation { [self] in
            teardown()
            self.urls = urls
            server = 0; attempts = 0; refusals = 0; kicks = 0
            set(.connecting)
            connect()
        }
    }

    func stop() {
        queue.addOperation { [self] in teardown(); urls = []; set(.idle) }
    }

    private func set(_ s: RadioState) {
        guard s != state else { return }
        state = s
        onState(s)
    }

    private func teardown() {
        generation += 1
        retry?.cancel(); retry = nil
        task?.cancel(); task = nil
        closeParser()
        if wired { node.stop() }
        queued = 0; started = false
    }

    private func closeParser() {
        if let parser { AudioFileStreamClose(parser) }
        parser = nil; converter = nil
    }

    private func connect() {
        guard !urls.isEmpty else { return }
        do { try startEngine() } catch { set(.refused); return }
        connectedAt = nil; gotAudio = false
        task = session.dataTask(with: urls[server % urls.count])
        task?.resume()
    }

    private func startEngine() throws {
        if !wired {
            engine.attach(node)
            engine.connect(node, to: engine.mainMixerNode, format: pcm)
            node.installTap(onBus: 0, bufferSize: 2048, format: pcm) { [meter] b, _ in meter.feed(b) }
            engineObserver = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: queue) { [weak self] _ in
                self?.outputChanged()
            }
            wired = true
        }
        if !engine.isRunning { try engine.start() }
    }

    /// AirPods in or out stops the engine. Scheduled audio is gone with it, so reconnect.
    private func outputChanged() {
        guard !urls.isEmpty, !engine.isRunning else { return }
        teardown()
        set(.connecting)
        connect()
    }

    // MARK: Network

    func urlSession(_ s: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse, completionHandler: @escaping (URLSession.ResponseDisposition) -> Void) {
        guard dataTask === task else { return completionHandler(.cancel) }
        let http = response as? HTTPURLResponse
        guard http?.statusCode == 200 else {
            completionHandler(.cancel)
            task = nil
            if http?.statusCode == 401 || http?.statusCode == 403 { wasRefused() } else { scheduleRetry(nextServer: true) }
            return
        }
        connectedAt = .now
        attempts = 0
        // Icecast says what it sends: MP3 on the 320k streams, AAC on the others.
        let aac = http?.value(forHTTPHeaderField: "Content-Type")?.contains("aac") == true
        closeParser()
        let me = Unmanaged.passUnretained(self).toOpaque()
        AudioFileStreamOpen(me, { me, stream, prop, _ in
            Unmanaged<Radio>.fromOpaque(me).takeUnretainedValue().property(stream, prop)
        }, { me, bytes, packets, data, descs in
            Unmanaged<Radio>.fromOpaque(me).takeUnretainedValue().packets(bytes, packets, data, descs)
        }, aac ? kAudioFileAAC_ADTSType : kAudioFileMP3Type, &parser)
        completionHandler(.allow)
    }

    func urlSession(_ s: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        guard dataTask === task, let parser else { return }
        data.withUnsafeBytes { _ = AudioFileStreamParseBytes(parser, UInt32($0.count), $0.baseAddress, []) }
    }

    func urlSession(_ s: URLSession, task t: URLSessionTask, didCompleteWithError error: Error?) {
        guard t === task else { return }
        task = nil
        closeParser()
        if let at = connectedAt, gotAudio, Date.now.timeIntervalSince(at) < kickWindow {
            kicks += 1
            if kicks >= kickLimit { return wasKicked() }
        } else if connectedAt != nil {
            kicks = 0
        }
        scheduleRetry(nextServer: connectedAt == nil)
    }

    /// The key is refused or keeps getting cut: most likely playing somewhere else. Retrying forever would fight that client.
    private func wasKicked() {
        generation += 1
        queued = 0; started = false
        node.stop()
        urls = []
        set(.refused)
    }

    private func wasRefused() {
        if refusals >= 1 { set(.refused) } else { set(.reconnecting) }
        guard refusals < refusedDelays.count else { urls = []; return }
        after(refusedDelays[refusals])
        refusals += 1
    }

    private func scheduleRetry(nextServer: Bool) {
        guard !urls.isEmpty else { return }
        if nextServer { server += 1 }
        if state != .refused { set(.reconnecting) }
        after(retryDelays[min(attempts, retryDelays.count - 1)])
        attempts += 1
    }

    private func after(_ delay: TimeInterval) {
        let g = generation
        let work = DispatchWorkItem { [weak self] in
            self?.queue.addOperation { [weak self] in
                guard let self, g == generation else { return }
                connect()
            }
        }
        retry = work
        DispatchQueue.global().asyncAfter(deadline: .now() + delay, execute: work)
    }

    // MARK: Decode

    private func property(_ stream: AudioFileStreamID, _ prop: AudioFileStreamPropertyID) {
        guard prop == kAudioFileStreamProperty_ReadyToProducePackets else { return }
        var asbd = AudioStreamBasicDescription()
        var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
        guard AudioFileStreamGetProperty(stream, kAudioFileStreamProperty_DataFormat, &size, &asbd) == noErr,
              let format = AVAudioFormat(streamDescription: &asbd) else { return }
        converter = AVAudioConverter(from: format, to: pcm)
    }

    private func packets(_ bytes: UInt32, _ count: UInt32, _ data: UnsafeRawPointer, _ descs: UnsafePointer<AudioStreamPacketDescription>?) {
        guard let converter, count > 0, bytes > 0 else { return }
        let input = AVAudioCompressedBuffer(format: converter.inputFormat, packetCapacity: count, maximumPacketSize: Int(bytes))
        input.data.copyMemory(from: data, byteCount: Int(bytes))
        input.byteLength = bytes
        input.packetCount = count
        if let descs, let out = input.packetDescriptions { out.update(from: descs, count: Int(count)) }

        let perPacket = max(converter.inputFormat.streamDescription.pointee.mFramesPerPacket, 1152)
        guard let output = AVAudioPCMBuffer(pcmFormat: pcm, frameCapacity: (count + 2) * perPacket) else { return }
        var fed = false
        var error: NSError?
        converter.convert(to: output, error: &error) { _, status in
            if fed { status.pointee = .noDataNow; return nil }
            fed = true
            status.pointee = .haveData
            return input
        }
        guard error == nil, output.frameLength > 0 else { return }
        schedule(output)
    }

    private func schedule(_ b: AVAudioPCMBuffer) {
        gotAudio = true
        refusals = 0
        let g = generation, n = b.frameLength
        queued += n
        node.scheduleBuffer(b) { [weak self] in
            self?.queue.addOperation { [weak self] in
                guard let self, g == generation else { return }
                queued -= min(n, queued)
                if started, queued < 2048 { // ran dry: hold until there is enough to play smoothly
                    started = false
                    node.pause()
                    set(task == nil ? .reconnecting : .buffering)
                }
            }
        }
        if !started, Double(queued) >= preroll * pcm.sampleRate {
            started = true
            node.play()
        }
        if started { set(.playing) } else if state != .refused { set(.buffering) }
    }
}
