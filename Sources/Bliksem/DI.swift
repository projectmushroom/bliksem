import Foundation

/// DI.fm image paths come as "//cdn-images…png"; the CDN resizes with ?width=.
func img(_ path: String?, _ width: Int) -> URL? {
    path.flatMap { URL(string: "https:\($0)?width=\(width)") }
}

struct Channel: Codable, Identifiable, Hashable {
    let id: Int
    let key: String
    let name: String
    let asset_url: String?
    var art: URL? { img(asset_url, 600) }
}

struct NowPlaying: Codable {
    struct Track: Codable {
        let id: Int
        let display_artist: String?
        let display_title: String?
        var artist: String { display_artist ?? "" }
        var title: String { display_title ?? "" }
    }
    let channel_id: Int
    let track: Track?
}

struct HistoryTrack: Codable, Identifiable {
    struct Votes: Codable { let up: Int; let down: Int }
    let track_id: Int
    let display_artist: String?
    let display_title: String?
    let started: Double?
    let art_url: String?
    let votes: Votes?
    var id: Int { track_id }
    var artist: String { display_artist ?? "" }
    var title: String { display_title ?? "" }
}

/// One bad element must not blank a whole list.
struct Lenient<T: Decodable>: Decodable {
    let value: T?
    init(from d: Decoder) { value = try? T(from: d) }
}

struct DIError: LocalizedError {
    let message: String
    init(_ body: String) { message = String(body.prefix(80)) }
    var errorDescription: String? { message }
}

enum DI {
    static func get<T: Decodable>(_ path: String) async throws -> T {
        let (data, _) = try await URLSession.shared.data(from: URL(string: "https://api.audioaddict.com/v1/di/" + path)!)
        return try JSONDecoder().decode(T.self, from: data)
    }

    static func list<T: Decodable>(_ path: String) async throws -> [T] {
        let l: [Lenient<T>] = try await get(path)
        return l.compactMap(\.value)
    }

    static func channels() async throws -> [Channel] { try await list("channels") }

    static func technoIDs() async throws -> [Int] {
        struct Filter: Decodable { let name: String; let channels: [Channel] }
        let fs: [Filter] = try await list("channel_filters")
        return fs.first { $0.name == "Techno" }?.channels.map(\.id) ?? []
    }

    static func nowPlaying() async throws -> [NowPlaying] { try await list("currently_playing") }

    static func history(_ channelID: Int) async throws -> [HistoryTrack] {
        try await list("track_history/channel/\(channelID)")
    }

    static func isrc(trackID: Int) async throws -> String? {
        struct T: Decodable { let isrc: String? }
        let t: T = try await get("tracks/\(trackID)")
        return t.isrc
    }

    /// Premium 320k. DI no longer serves free streams to third-party players.
    static func streamURL(channel: String, key: String) async throws -> URL {
        let (data, _) = try await URLSession.shared.data(from: URL(string: "https://listen.di.fm/premium_high/\(channel).pls?listen_key=\(key)")!)
        let body = String(decoding: data, as: UTF8.self)
        let urls = parsePLS(body)
        guard !urls.isEmpty else { throw DIError(body) }
        // DI lists several servers; some don't even resolve, so take the first that answers.
        for url in urls where await reachable(url) { return url }
        throw URLError(.cannotConnectToHost)
    }

    static func parsePLS(_ s: String) -> [URL] {
        s.split(whereSeparator: \.isNewline).filter { $0.hasPrefix("File") }.compactMap { line in
            line.firstIndex(of: "=").flatMap { URL(string: line[line.index(after: $0)...].trimmingCharacters(in: .whitespaces)) }
        }
    }

    static func reachable(_ url: URL) async -> Bool {
        guard let (bytes, response) = try? await URLSession.shared.bytes(for: URLRequest(url: url, timeoutInterval: 4)) else { return false }
        bytes.task.cancel()
        return (response as? HTTPURLResponse)?.statusCode == 200
    }
}

enum AppleMusic {
    struct Result: Decodable { let artistName: String; let trackViewUrl: String }
    private struct Response: Decodable { let results: [Result] }

    static var country: String { Locale.current.region?.identifier ?? "US" }

    /// ISRC is exact when DI has it (rare); otherwise artist + title search, first hit whose artist matches.
    static func find(isrc: String?, artist: String, title: String) async -> URL? {
        if let isrc, let r = await query("lookup?isrc=\(isrc)").first { return musicURL(r) }
        return pick(await query("search?term=\(enc(artist + " " + title))&entity=song&limit=5"), artist: artist)
    }

    static func pick(_ rs: [Result], artist: String) -> URL? {
        let a = artist.lowercased()
        return rs.first { let r = $0.artistName.lowercased(); return r.contains(a) || a.contains(r) }.flatMap(musicURL)
    }

    static func searchURL(_ q: String) -> URL {
        URL(string: "music://music.apple.com/\(country.lowercased())/search?term=\(enc(q))")!
    }

    static func enc(_ s: String) -> String { s.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? "" }

    private static func query(_ q: String) async -> [Result] {
        guard let url = URL(string: "https://itunes.apple.com/\(q)&country=\(country)"),
              let (d, _) = try? await URLSession.shared.data(from: url) else { return [] }
        return (try? JSONDecoder().decode(Response.self, from: d))?.results ?? []
    }

    private static func musicURL(_ r: Result) -> URL? {
        URL(string: r.trackViewUrl.replacingOccurrences(of: "https://", with: "music://"))
    }
}

enum Links {
    static func all(for q: String) -> [(String, URL)] {
        let e = AppleMusic.enc(q)
        return [
            ("Beatport", "https://www.beatport.com/search?q=\(e)"),
            ("Bandcamp", "https://bandcamp.com/search?q=\(e)"),
            ("Discogs", "https://www.discogs.com/search/?q=\(e)"),
            ("YouTube", "https://www.youtube.com/results?search_query=\(e)"),
        ].map { ($0.0, URL(string: $0.1)!) }
    }
}
