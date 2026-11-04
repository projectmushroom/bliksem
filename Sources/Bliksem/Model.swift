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

struct ChannelSection: Identifiable {
    let id: String
    let title: String
    let channels: [Channel]
}

@MainActor @Observable final class Model {
    static let favSection = "favs"
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
    private var wanted = false
    var status = ""
    var query = ""
    var focusSearch = 0
    var focusRoot = 0
    var favs: [Int] = load("favs") ?? [] { didSet { save(favs, "favs") } }
    var collapsed: Set<String> = load("collapsed") ?? [] { didSet { save(collapsed, "collapsed") } }
    var slots: [Module] = load("slots") ?? [.vu, .wave, .last] { didSet { save(slots, "slots") } }

    let radio: Radio
    /// Lives here so the needles keep swinging when a module is flipped away and back.
    @ObservationIgnored let needle = Ballistics()
    private var ticks = 0
    private var lastTrackID: Int?

    var listenKey: String = UserDefaults.standard.string(forKey: "key") ?? "" { didSet { UserDefaults.standard.set(listenKey, forKey: "key") } }
    var zoom: CGFloat = max(1, CGFloat(UserDefaults.standard.double(forKey: "zoom"))) { didSet { UserDefaults.standard.set(zoom, forKey: "zoom") } }

    var key: String { listenKey.trimmingCharacters(in: .whitespacesAndNewlines) }

    func zoomIn() { zoom = Phosphor.zooms.first { $0 > zoom } ?? zoom }
    func zoomOut() { zoom = Phosphor.zooms.last { $0 < zoom } ?? zoom }
    func zoomReset() { zoom = 1 }
    var nowTrack: NowPlaying.Track? { current.flatMap { onAir[$0.id] } }
    var nowArtPath: String? { history.first { $0.track_id == nowTrack?.id }?.art_url }
    var nowArt: URL? { img(nowArtPath, 600) ?? current?.art }

    /// Favourites on top, DI's techno group next, everything else after. A search flattens it to the matches.
    var sections: [ChannelSection] {
        let q = query.lowercased()
        if !q.isEmpty {
            return [ChannelSection(id: "results", title: Copy.results, channels: channels.filter { $0.name.lowercased().contains(q) || $0.key.contains(q) })]
        }
        let byID = Dictionary(channels.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let pinned = favs.compactMap { byID[$0] }
        let techno = technoIDs.filter { !favs.contains($0) }.compactMap { byID[$0] }
        let taken = Set(favs + technoIDs)
        let rest = channels.filter { !taken.contains($0.id) }.sorted { $0.name < $1.name }
        return [
            ChannelSection(id: Self.favSection, title: Copy.favs, channels: pinned),
            ChannelSection(id: "techno", title: Copy.techno, channels: techno),
            ChannelSection(id: "rest", title: Copy.rest, channels: rest),
        ]
    }

    /// What the arrow keys hop through: the channels you can see.
    var visible: [Channel] { sections.filter { !collapsed.contains($0.id) }.flatMap(\.channels) }

    /// Cmd+1-9 and the menu bar: favourites first, then whatever you listen to most.
    var top: [Channel] {
        let byID = Dictionary(channels.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let listened = channels.filter { seconds[$0.key, default: 0] > 0 && !favs.contains($0.id) }
            .sorted { seconds[$0.key, default: 0] > seconds[$1.key, default: 0] }
        return Array((favs.compactMap { byID[$0] } + listened).prefix(9))
    }

    func isFav(_ id: Int) -> Bool { favs.contains(id) }
    func toggleFav(_ id: Int) { if let i = favs.firstIndex(of: id) { favs.remove(at: i) } else { favs.append(id) } }
    func toggleCollapsed(_ id: String) { if collapsed.contains(id) { collapsed.remove(id) } else { collapsed.insert(id) } }

    func cycle(slot: Int, by d: Int) {
        let all = Module.allCases
        guard slots.indices.contains(slot), let i = all.firstIndex(of: slots[slot]) else { return }
        // Skip modules already showing in another slot.
        for step in 1..<all.count {
            let next = all[(i + d * step + all.count * step) % all.count]
            if !slots.contains(next) { slots[slot] = next; return }
        }
    }
    func addSlot() { if let free = Module.allCases.first(where: { !slots.contains($0) }) { slots.append(free) } }
    func removeSlot(_ i: Int) { if slots.count > 1, slots.indices.contains(i) { slots.remove(at: i) } }

    init(radio: Radio = Radio(), autoplay: Bool = true) {
        self.radio = radio
        radio.onState = { [weak self] s in DispatchQueue.main.async { self?.radioChanged(s) } }
        remote()
        Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tick() }
        }
        Task {
            await refreshChannels()
            await refresh()
            if autoplay, let id = UserDefaults.standard.object(forKey: "lastChannel") as? Int,
               let c = channels.first(where: { $0.id == id }) {
                play(c)
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
        guard !key.isEmpty else { status = Copy.noKey; return }
        Task {
            do {
                let urls = try await DI.streamURLs(channel: c.key, key: key)
                guard current?.id == c.id else { return }
                wanted = true
                radio.play(urls)
                await refresh()
            } catch {
                status = error is DIError ? Copy.badKey : Copy.dead
            }
        }
    }

    func radioChangedForTests(_ s: RadioState) { radioChanged(s) }

    private func radioChanged(_ s: RadioState) {
        playing = s == .playing
        switch s {
        case .idle, .playing: status = ""
        case .connecting, .buffering: status = Copy.loading
        case .reconnecting: status = Copy.dropped
        case .refused: status = Copy.refused; wanted = false
        }
        MPNowPlayingInfoCenter.default().playbackState = wanted ? .playing : .paused
    }

    /// Radio: stop drops the buffer, play reconnects live.
    func stop() {
        wanted = false
        radio.stop()
        status = ""
    }

    func toggle() {
        if !wanted {
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
        if playing {
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
            (r.playCommand, { [weak self] in if self?.wanted == false { self?.toggle() } }),
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
