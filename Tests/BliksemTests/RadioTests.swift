import AVFoundation
import Network
import XCTest
@testable import Bliksem

/// A local stand-in for DI's Icecast, so playback tests never touch a real key or stream.
final class FakeIcecast {
    enum Rule { case serve, refuse }
    var rule: (Int) -> Rule = { _ in .serve }
    /// Server cuts each connection after this many seconds.
    var cutAfter: TimeInterval?
    private(set) var connections = 0
    private(set) var maxConcurrent = 0
    private var active = 0
    private let listener: NWListener
    private let q = DispatchQueue(label: "fake-icecast")
    private let audio: Data
    var url: URL { URL(string: "http://127.0.0.1:\(listener.port!.rawValue)/techno_hi")! }

    init(audio: Data) throws {
        self.audio = audio
        listener = try NWListener(using: .tcp, on: .any)
        listener.newConnectionHandler = { [unowned self] c in self.accept(c) }
        let ready = DispatchSemaphore(value: 0)
        listener.stateUpdateHandler = { if case .ready = $0 { ready.signal() } }
        listener.start(queue: q)
        ready.wait()
    }
    deinit { listener.cancel() }

    private func accept(_ c: NWConnection) {
        c.start(queue: q)
        c.receive(minimumIncompleteLength: 1, maximumLength: 8192) { [unowned self] _, _, _, _ in
            connections += 1
            if rule(connections) == .refuse {
                c.send(content: Data("HTTP/1.0 401 Authentication Required\r\nContent-Length: 0\r\n\r\n".utf8), completion: .contentProcessed { _ in c.cancel() })
                return
            }
            active += 1; maxConcurrent = max(maxConcurrent, active)
            var head = Data("HTTP/1.0 200 OK\r\nContent-Type: audio/aacp\r\n\r\n".utf8)
            head.append(audio.prefix(32000)) // Icecast's 2s burst
            c.send(content: head, completion: .idempotent)
            let open = Open()
            watchClose(c, open)
            pump(c, open, sent: 32000, start: Date())
        }
    }

    private final class Open { var value = true }

    /// A pending receive completes the moment the client hangs up, so `active` is exact.
    private func watchClose(_ c: NWConnection, _ open: Open) {
        c.receive(minimumIncompleteLength: 1, maximumLength: 1024) { [weak self] _, _, done, error in
            guard let self else { return }
            if done || error != nil {
                if open.value { open.value = false; self.active -= 1 }
                c.cancel()
            } else { self.watchClose(c, open) }
        }
    }

    private func pump(_ c: NWConnection, _ open: Open, sent: Int, start: Date) {
        q.asyncAfter(deadline: .now() + 0.05) { [weak self] in
            guard let self else { return }
            let elapsed = Date().timeIntervalSince(start)
            guard open.value else { return }
            if self.cutAfter.map({ elapsed > $0 }) ?? false { open.value = false; self.active -= 1; c.cancel(); return }
            var sent = sent
            let want = 32000 + Int(elapsed * 16000)
            var out = Data()
            while sent < want {
                let off = sent % self.audio.count
                let piece = self.audio.subdata(in: off..<min(off + want - sent, self.audio.count))
                out.append(piece); sent += piece.count
            }
            c.send(content: out, completion: .idempotent)
            self.pump(c, open, sent: sent, start: start)
        }
    }
}

final class RadioTests: XCTestCase {
    /// 20s of pulsing tone as ADTS AAC, the format the fake server sends.
    static let fixture: Data = {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try! FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let wav = dir.appendingPathComponent("t.caf"), aac = dir.appendingPathComponent("t.aac")
        let fmt = AVAudioFormat(standardFormatWithSampleRate: 44100, channels: 2)!
        let file = try! AVAudioFile(forWriting: wav, settings: fmt.settings)
        let n = 44100 * 20
        let b = AVAudioPCMBuffer(pcmFormat: fmt, frameCapacity: AVAudioFrameCount(n))!
        b.frameLength = AVAudioFrameCount(n)
        for i in 0..<n {
            let env: Float = i % 22050 < 6000 ? 1 : 0.15
            let phase: Float = 2 * Float.pi * 110 * Float(i) / 44100
            let v: Float = 0.6 * env * sin(phase)
            b.floatChannelData![0][i] = v; b.floatChannelData![1][i] = v
        }
        try! file.write(from: b)
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/afconvert")
        p.arguments = ["-f", "adts", "-d", "aac", "-b", "128000", wav.path, aac.path]
        try! p.run(); p.waitUntilExit()
        return try! Data(contentsOf: aac)
    }()

    private var server: FakeIcecast!
    private var radio: Radio!
    private let lock = NSLock()
    private var states: [RadioState] = []

    override func setUpWithError() throws {
        server = try FakeIcecast(audio: Self.fixture)
        radio = Radio()
        radio.muted = true
        radio.retryDelays = [0.1, 0.2]
        radio.refusedDelays = [0.1, 0.1, 0.1]
        states = []
        radio.onState = { [unowned self] s in lock.lock(); states.append(s); lock.unlock() }
    }
    override func tearDown() { radio.stop(); radio = nil; server = nil }

    private var state: RadioState { lock.lock(); defer { lock.unlock() }; return states.last ?? .idle }
    private func wait(_ seconds: TimeInterval) { RunLoop.current.run(until: Date().addingTimeInterval(seconds)) }
    private func waitFor(_ s: RadioState, within: TimeInterval) {
        let end = Date().addingTimeInterval(within)
        while state != s, Date() < end { wait(0.05) }
        XCTAssertEqual(state, s, "states seen: \(states)")
    }

    /// The bug: playback died after about 10 seconds. This must outlast it, on one single socket.
    func testKeepsPlayingPastTenSecondsOnOneConnection() {
        radio.play([server.url])
        waitFor(.playing, within: 3)
        wait(15)
        XCTAssertEqual(state, .playing, "states seen: \(states)")
        XCTAssertEqual(server.connections, 1, "one stream per key: a second connection could get us kicked")
        XCTAssertGreaterThan(radio.rendered, 13, "audio must really have rendered, not just connected")
    }

    func testMeterSeesTheAudio() {
        radio.play([server.url])
        waitFor(.playing, within: 3)
        wait(3)
        let levels = radio.meter.recent(40)
        XCTAssertGreaterThan(levels.count, 20)
        XCTAssertGreaterThan(levels.max() ?? 0, 0.2, "levels must reflect the tone")
        XCTAssertLessThan(levels.min() ?? 1, (levels.max() ?? 0) * 0.6, "the pulse must be visible, not a flat line")
    }

    func testReconnectsWhenCutThenKeepsPlaying() {
        server.cutAfter = 3
        radio.play([server.url])
        waitFor(.playing, within: 3)
        wait(5)
        XCTAssertGreaterThanOrEqual(server.connections, 2, "must reconnect by itself")
        server.cutAfter = nil
        waitFor(.playing, within: 4)
    }

    func testStopsFightingWhenKeptBeingKicked() {
        server.cutAfter = 1
        radio.kickLimit = 3
        radio.play([server.url])
        waitFor(.refused, within: 15)
        let seen = server.connections
        XCTAssertEqual(seen, 3)
        wait(2)
        XCTAssertEqual(server.connections, seen, "must stop reconnecting once it gives up")
    }

    func testRefusedKeyGivesUpWithoutHammering() {
        server.rule = { _ in .refuse }
        radio.play([server.url])
        waitFor(.refused, within: 3)
        wait(1.5)
        XCTAssertEqual(server.connections, 4, "first try plus exactly the configured retries")
    }

    func testRecoversWhenRefusalClears() {
        server.rule = { $0 == 1 ? .refuse : .serve } // the old session still being open when we hop channels
        radio.play([server.url])
        waitFor(.playing, within: 4)
    }

    func testFallsBackToSecondServer() {
        radio.play([URL(string: "http://127.0.0.1:9/dead")!, server.url])
        waitFor(.playing, within: 5)
    }

    func testHoppingNeverOverlapsConnections() {
        for _ in 0..<4 { radio.play([server.url]); wait(0.4) }
        waitFor(.playing, within: 3)
        wait(1)
        XCTAssertEqual(server.maxConcurrent, 1, "the old stream must be closed before the next one opens")
    }

    func testStopClosesTheSocketAndStaysStopped() {
        radio.play([server.url])
        waitFor(.playing, within: 3)
        radio.stop()
        waitFor(.idle, within: 1)
        wait(1)
        XCTAssertEqual(server.connections, 1)
    }

    /// Real DI, real key, muted, through the app's own logic. Only runs when asked, because it takes your one stream
    /// and kicks any other device on that key: DI_LISTEN_KEY=... DI_SOAK=90 swift test --filter testRealStreamSoak
    @MainActor func testRealStreamSoak() async throws {
        let env = ProcessInfo.processInfo.environment
        guard let key = env["DI_LISTEN_KEY"], !key.isEmpty, let secs = env["DI_SOAK"].flatMap(Double.init) else { throw XCTSkip("set DI_LISTEN_KEY and DI_SOAK") }
        let m = Model(radio: radio, autoplay: false)
        radio.onState = { [weak m, unowned self] s in
            lock.lock(); states.append(s); lock.unlock()
            DispatchQueue.main.async { m?.radioChangedForTests(s) }
        }
        m.listenKey = key
        defer { m.listenKey = "" }
        await m.refreshChannels()
        let channel = try XCTUnwrap(m.channels.first { $0.key == (env["DI_CHANNEL"] ?? "techno") })
        let before = m.seconds[channel.key] ?? 0
        m.play(channel)
        var levels: [Float] = []
        for _ in 0..<Int(secs) {
            try await Task.sleep(for: .seconds(1))
            levels += radio.meter.recent(20)
        }
        let n = levels.map(meterLevel).sorted()
        print("soak: rendered \(radio.rendered)s states \(states) status '\(m.status)' track '\(m.nowTrack?.artist ?? "-") - \(m.nowTrack?.title ?? "-")'")
        print("soak: meter level p05 \(n[n.count / 20]) p50 \(n[n.count / 2]) p95 \(n[n.count * 19 / 20]) max \(n.last ?? 0)")
        XCTAssertTrue(m.playing, "states seen: \(states)")
        XCTAssertEqual(m.status, "")
        XCTAssertGreaterThan(radio.rendered, secs - 5)
        XCTAssertNotNil(m.nowTrack)
        XCTAssertGreaterThan((m.seconds[channel.key] ?? 0) - before, Int(secs) - 8, "listening time must count")
        m.stop()
    }
}
