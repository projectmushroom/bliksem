import AVFoundation
import CoreText
import XCTest
@testable import Bliksem

final class BliksemTests: XCTestCase {
    func testParsePLS() {
        let pls = "[playlist]\nNumberOfEntries=2\nFile1=http://prem1.di.fm:80/techno_hi?abc\nTitle1=DI.FM - Techno\nFile2=http://prem2.di.fm:80/techno_hi?abc\n"
        XCTAssertEqual(DI.parsePLS(pls).map(\.absoluteString), ["http://prem1.di.fm:80/techno_hi?abc", "http://prem2.di.fm:80/techno_hi?abc"])
        XCTAssertTrue(DI.parsePLS("Invalid Listen Key").isEmpty)
    }

    func testDecodeNowPlayingSkipsBrokenEntries() throws {
        let json = #"[{"channel_id":36,"channel_key":"techno","track":{"id":1,"display_artist":"A","display_title":"T"}},{"nope":1}]"#
        let l = try JSONDecoder().decode([Lenient<NowPlaying>].self, from: Data(json.utf8)).compactMap(\.value)
        XCTAssertEqual(l.count, 1)
        XCTAssertEqual(l.first?.track?.artist, "A")
    }

    func testAppleMatchPrefersArtist() {
        let rs = [
            AppleMusic.Result(artistName: "Someone Else", trackViewUrl: "https://x/1"),
            AppleMusic.Result(artistName: "CC Luna", trackViewUrl: "https://x/2"),
        ]
        XCTAssertEqual(AppleMusic.pick(rs, artist: "cc luna")?.absoluteString, "music://x/2")
        XCTAssertNil(AppleMusic.pick(rs, artist: "nobody"))
    }

    func testPad() {
        XCTAssertEqual(pad("abc", 5), "abc  ")
        XCTAssertEqual(pad("abcdef", 3), "abc")
        XCTAssertEqual(pad("x", 0), "")
    }

    func testFontBundledWithBoxGlyphs() throws {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "Px437_IBM_VGA_9x16", withExtension: "ttf"))
        CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
        let font = CTFontCreateWithName("Px437 IBM VGA 9x16" as CFString, 16, nil)
        for ch in "╔═╗║╚╝─│█░▲▼↑↓" {
            var u = Array(ch.utf16)
            var g = [CGGlyph](repeating: 0, count: u.count)
            XCTAssertTrue(CTFontGetGlyphsForCharacters(font, &u, &g, u.count), "missing glyph \(ch)")
        }
        XCTAssertEqual(CTFontGetAscent(font) + CTFontGetDescent(font) + CTFontGetLeading(font), 16)
    }

    func testTechnoFilterFromLiveAPI() async throws {
        let ids = try await DI.technoIDs()
        XCTAssertEqual(ids.count, 14)
    }

    @MainActor func testModelLoadsTechnoList() async throws {
        let m = Model()
        for _ in 0..<40 where m.visible.isEmpty { try await Task.sleep(for: .milliseconds(250)) }
        XCTAssertEqual(m.visible.count, 14)
        XCTAssertGreaterThan(m.channels.count, 100)
    }

    /// Network smoke test, muted. Run with DI_LISTEN_KEY=… swift test
    func testPremiumStreamIsPlayable() async throws {
        guard let key = ProcessInfo.processInfo.environment["DI_LISTEN_KEY"], !key.isEmpty else { throw XCTSkip("set DI_LISTEN_KEY") }
        let url = try await DI.streamURL(channel: "techno", key: key)
        let player = AVPlayer(url: url)
        player.isMuted = true
        player.play()
        for _ in 0..<40 where player.currentItem?.status != .readyToPlay {
            try await Task.sleep(for: .milliseconds(250))
        }
        XCTAssertEqual(player.currentItem?.status, .readyToPlay, "\(url) \(String(describing: player.currentItem?.error))")
        player.pause()
    }
}
