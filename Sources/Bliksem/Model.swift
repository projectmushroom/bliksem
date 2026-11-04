import AVFoundation
import AppKit
import MediaPlayer
import Observation

struct Liked: Codable, Identifiable {
    let id: Int
    let artist: String
    let title: String
    let channel: String
    let date: Date
    let art: String?
    var apple: String?
    var appleState = "searching"
}

struct Heard: Codable, Identifiable {
    var id = UUID()
    let date: Date
    let channel: String
    let trackID: Int
    let artist: String
    let title: String
    let art: String?
}

private func load<T: Decodable>(_ key: String) -> T? {
    UserDefaults.standard.data(forKey: key).flatMap { try? JSONDecoder().decode(T.self, from: $0) }
}

private func save<T: Encodable>(_ value: T, _ key: String) {
    UserDefaults.standard.set(try? JSONEncoder().encode(value), forKey: key)
}

@MainActor @Observable final class Model {
    var channels: [Channel] = load("channels") ?? [] { didSet { save(channels, "channels") } }
    var technoIDs: [Int] = load("technoIDs") ?? [] { didSet { save(technoIDs, "technoIDs") } }
    var likes: [Liked] = load("likes") ?? [] { didSet { save(likes, "likes") } }
    // shortcut: heard log capped at 5000 tracks in UserDefaults, move it to a file when you want it forever
    var heard: [Heard] = load("heard") ?? [] { didSet { save(heard.suffix(5000), "heard") } }
    var seconds: [String: Int] = load("seconds") ?? [:] { didSet { save(seconds, "seconds") } }
    var onAir: [Int: NowPlaying.Track] = [:]
    var history: [HistoryTrack] = []
    var current: Channel?
    var playing = false
    var status = ""
    var query = ""
    var showAll = false
    var focusSearch = 0

    private let player = AVPlayer()
    private var ticks = 0
    private var lastTrackID: Int?
    private var playerObs: NSKeyValueObservation?
    private var itemObs: NSKeyValueObservation?

    var key: String {
        (UserDefaults.standard.string(forKey: "key") ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    }
    var nowTrack: NowPlaying.Track? { current.flatMap { onAir[$0.id] } }
    var nowArtPath: String? { history.first { $0.track_id == nowTrack?.id }?.art_url }
    var nowArt: URL? { img(nowArtPath, 600) ?? current?.art }

    var visible: [Channel] {
        let q = query.lowercased()
        if !q.isEmpty { return channels.filter { $0.name.lowercased().contains(q) || $0.key.contains(q) } }
        if showAll { return channels }
        return technoIDs.compactMap { id in channels.first { $0.id == id } }
    }

    var top: [Channel] {
        Array(channels.filter { seconds[$0.key, default: 0] > 0 }
            .sorted { seconds[$0.key, default: 0] > seconds[$1.key, default: 0] }
            .prefix(9))
    }

    init() {
        playerObs = player.observe(\.timeControlStatus) { [weak self] p, _ in
            let s = p.timeControlStatus
            Task { @MainActor in
                guard let self else { return }
                self.playing = s == .playing
                if s == .playing { self.status = "" } else if s == .waitingToPlayAtSpecifiedRate { self.status = Copy.loading }
                MPNowPlayingInfoCenter.default().playbackState = s == .paused ? .paused : .playing
            }
        }
        remote()
        Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tick() }
        }
        Task {
            await refreshChannels()
            if let id = UserDefaults.standard.object(forKey: "lastChannel") as? Int,
               let c = channels.first(where: { $0.id == id }) {
                play(c)
            } else {
                await refresh()
            }
        }
    }

    func play(_ c: Channel) {
        current = c
        lastTrackID = nil
        history = []
        status = Copy.loading
        UserDefaults.standard.set(c.id, forKey: "lastChannel")
        nowPlayingInfo()
        Task {
            do {
                let url = try await DI.streamURL(channel: c.key, key: key)
                guard current?.id == c.id else { return }
                let item = AVPlayerItem(url: url)
                itemObs = item.observe(\.status) { [weak self] i, _ in
                    if i.status == .failed { Task { @MainActor in self?.status = Copy.dead } }
                }
                player.replaceCurrentItem(with: item)
                player.play()
                await refresh()
            } catch {
                status = error is DIError ? Copy.badKey : Copy.dead
            }
        }
    }

    /// Radio: stop drops the buffer, play reconnects live.
    func stop() {
        player.pause()
        player.replaceCurrentItem(with: nil)
        status = ""
    }

    func toggle() {
        if player.timeControlStatus == .paused {
            if let c = current ?? visible.first { play(c) }
        } else {
            stop()
        }
    }

    func hop(_ d: Int) {
        let v = visible
        guard !v.isEmpty else { return }
        let i = v.firstIndex { $0.id == current?.id } ?? -1
        play(v[(i + d + v.count) % v.count])
    }

    func isLiked(_ id: Int) -> Bool { likes.contains { $0.id == id } }

    func toggleLike(id: Int, artist: String, title: String, channel: String, art: String?) {
        if let i = likes.firstIndex(where: { $0.id == id }) { likes.remove(at: i); return }
        likes.insert(Liked(id: id, artist: artist, title: title, channel: channel, date: .now, art: art), at: 0)
        Task {
            let isrc = (try? await DI.isrc(trackID: id)) ?? nil
            let url = await AppleMusic.find(isrc: isrc, artist: artist, title: title)
            guard let i = likes.firstIndex(where: { $0.id == id }) else { return }
            likes[i].apple = url?.absoluteString
            likes[i].appleState = url == nil ? "none" : "found"
        }
    }

    func likeCurrent() {
        guard let t = nowTrack, let c = current else { return }
        toggleLike(id: t.id, artist: t.artist, title: t.title, channel: c.name, art: nowArtPath)
    }

    func refreshChannels() async {
        if let c = try? await DI.channels(), !c.isEmpty { channels = c }
        if let t = try? await DI.technoIDs(), !t.isEmpty { technoIDs = t }
    }

    func refresh() async {
        if let np = try? await DI.nowPlaying() {
            onAir = Dictionary(np.compactMap { e in e.track.map { (e.channel_id, $0) } }, uniquingKeysWith: { a, _ in a })
        }
        guard let c = current else { return }
        let h = (try? await DI.history(c.id)) ?? []
        guard current?.id == c.id else { return }
        history = h
        guard let t = nowTrack, t.id != lastTrackID else { return }
        lastTrackID = t.id
        if player.timeControlStatus != .paused {
            heard.append(Heard(date: .now, channel: c.name, trackID: t.id, artist: t.artist, title: t.title, art: nowArtPath))
        }
        nowPlayingInfo()
    }

    private func tick() {
        ticks += 1
        if playing, let c = current { seconds[c.key, default: 0] += 1 }
        if ticks % 15 == 0 { Task { await refresh() } }
    }

    /// System-wide Now Playing: Control Center, media keys, AirPods.
    private func nowPlayingInfo() {
        let center = MPNowPlayingInfoCenter.default()
        guard let c = current else { center.nowPlayingInfo = nil; return }
        var info: [String: Any] = [
            MPNowPlayingInfoPropertyIsLiveStream: true,
            MPMediaItemPropertyTitle: nowTrack?.title ?? c.name,
            MPMediaItemPropertyArtist: nowTrack?.artist ?? "DI.FM",
            MPMediaItemPropertyAlbumTitle: "DI.FM \(c.name)",
        ]
        center.nowPlayingInfo = info
        guard let url = nowArt else { return }
        Task {
            guard let (d, _) = try? await URLSession.shared.data(from: url), let image = NSImage(data: d),
                  center.nowPlayingInfo?[MPMediaItemPropertyTitle] as? String == info[MPMediaItemPropertyTitle] as? String
            else { return }
            info[MPMediaItemPropertyArtwork] = MPMediaItemArtwork(boundsSize: image.size) { _ in image }
            center.nowPlayingInfo = info
        }
    }

    private func remote() {
        let r = MPRemoteCommandCenter.shared()
        let actions: [(MPRemoteCommand, @MainActor () -> Void)] = [
            (r.playCommand, { [weak self] in if self?.player.timeControlStatus == .paused { self?.toggle() } }),
            (r.pauseCommand, { [weak self] in self?.stop() }),
            (r.togglePlayPauseCommand, { [weak self] in self?.toggle() }),
            (r.nextTrackCommand, { [weak self] in self?.hop(1) }),
            (r.previousTrackCommand, { [weak self] in self?.hop(-1) }),
        ]
        for (cmd, act) in actions {
            cmd.addTarget { _ in Task { @MainActor in act() }; return .success }
        }
    }
}
