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
        for ch in "╔═╗║╚╝─│█░▀▄►▲▼↑↓" {
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

    private func channel(_ id: Int, _ name: String) -> Channel { Channel(id: id, key: name.lowercased(), name: name, asset_url: nil) }

    @MainActor private func model() -> Model {
        let m = Model(autoplay: false)
        m.favs = []; m.collapsed = []; m.query = ""; m.slots = [.vu, .wave, .last]
        m.channels = [channel(1, "Techno"), channel(2, "Minimal"), channel(3, "Ambient"), channel(4, "Bass")]
        m.technoIDs = [1, 2]
        return m
    }

    @MainActor func testSectionsListEveryChannelWithFavouritesOnTop() {
        let m = model()
        m.toggleFav(4); m.toggleFav(2)
        XCTAssertEqual(m.sections.map(\.id), [Model.favSection, "techno", "rest"])
        XCTAssertEqual(m.sections[0].channels.map(\.id), [4, 2], "pin order")
        XCTAssertEqual(m.sections[1].channels.map(\.id), [1], "a favourite moves out of its group")
        XCTAssertEqual(m.sections[2].channels.map(\.id), [3])
        XCTAssertEqual(Set(m.sections.flatMap(\.channels).map(\.id)), [1, 2, 3, 4], "nothing lost, nothing doubled")
    }

    @MainActor func testCollapsedSectionLeavesHopOrder() {
        let m = model()
        m.toggleFav(4)
        m.toggleCollapsed(Model.favSection)
        XCTAssertEqual(m.visible.map(\.id), [1, 2, 3])
        m.query = "amb"
        XCTAssertEqual(m.sections.flatMap(\.channels).map(\.id), [3])
    }

    @MainActor func testTopPrefersFavourites() {
        let m = model()
        m.seconds = ["ambient": 500, "minimal": 100]
        m.favs = [4]
        XCTAssertEqual(m.top.map(\.id), [4, 3, 2])
    }

    @MainActor func testSlotCyclingNeverDuplicatesAModule() {
        let m = model()
        m.cycle(slot: 0, by: 1)
        XCTAssertEqual(Set(m.slots).count, m.slots.count)
        XCTAssertFalse(m.slots.contains(.vu) && m.slots[0] == .vu, "must have moved on")
        m.cycle(slot: 0, by: -1)
        XCTAssertEqual(Set(m.slots).count, m.slots.count)
        m.removeSlot(0); m.removeSlot(0); m.removeSlot(0)
        XCTAssertEqual(m.slots.count, 1, "the last slot stays")
        m.addSlot()
        XCTAssertEqual(m.slots.count, 2)
    }

    func testFitModulesKeepsWholeCellsAndFillsTheWidth() {
        let fit = fitModules([.vu, .wave, .last], cols: 98)
        XCTAssertEqual(fit.map(\.module), [.vu, .wave, .last])
        XCTAssertEqual(fit.map(\.cols).reduce(0, +), 98)
        XCTAssertEqual(fit[0].cols, 20)
        XCTAssertEqual(fitModules([.vu, .wave, .last], cols: 76).map(\.module), [.vu, .wave, .last], "the old default window still fits three")
        XCTAssertEqual(fitModules([.vu, .wave, .last], cols: 60).map(\.module), [.vu, .wave], "drops what doesn't fit")
        XCTAssertEqual(fitModules([.vu, .wave, .last], cols: 60).map(\.cols).reduce(0, +), 60)
        XCTAssertEqual(fitModules([.vu], cols: 50).map(\.cols), [50], "a lone meter takes the whole bay")
        XCTAssertEqual(fitModules([.last], cols: 10).map(\.module), [.last], "too narrow still shows something")
    }

    func testMeterScale() {
        XCTAssertEqual(meterLevel(0), 0)
        XCTAssertEqual(meterLevel(1), 1, accuracy: 0.001)
        XCTAssertEqual(meterLevel(2), 1, "clips at the top")
        XCTAssertEqual(meterLevel(0.0001), 0, "below the floor")
        XCTAssertEqual(meterLevel(0.0158), 0, accuracy: 0.01, "-36 dB is the floor")
    }

    func testWaveRowsMirrorAroundTheCentre() {
        let rows = waveRows([1, 0], cols: 3, rows: 4)
        XCTAssertEqual(rows.count, 4)
        XCTAssertTrue(rows.allSatisfy { $0.count == 3 })
        XCTAssertEqual(rows.map { String(Array($0)[1]) }, ["█", "█", "█", "█"], "full level fills the column")
        XCTAssertEqual(rows.map { String(Array($0)[0]) }, [" ", " ", " ", " "], "history pads from the left")
        XCTAssertEqual(rows.map { String(Array($0)[2]) }, [" ", " ", " ", " "], "silence draws nothing")
        let mid = waveRows([0.35], cols: 1, rows: 4).map { String($0) }
        XCTAssertEqual(mid, mid.reversed().map { $0 == "▀" ? "▄" : $0 == "▄" ? "▀" : $0 }, "mirrored")
    }

    func testWaveScaleSpreadsRealTechnoLoudness() {
        // The spread measured on the real Techno stream, as meterLevel values p05 / p50 / p95: 0.51 / 0.71 / 0.80.
        let amp = { (m: Float) in pow(10, (m * 36 - 36) / 20) }
        let (lo, mid, hi) = (waveLevel(amp(0.51)), waveLevel(amp(0.71)), waveLevel(amp(0.80)))
        XCTAssertLessThan(lo, 0.2, "quiet moments must look quiet")
        XCTAssertGreaterThan(hi, 0.7, "kicks must reach high")
        XCTAssertTrue(lo < mid && mid < hi)
    }

    func testNeedleSwingsUpThenFalls() {
        let meter = Meter(), needle = Ballistics()
        let until = Date().addingTimeInterval(0.4)
        while Date() < until { meter.feedDemo(); needle.step(meter); RunLoop.current.run(until: Date().addingTimeInterval(0.02)) }
        XCTAssertGreaterThan(needle.l, 0.2, "must rise with the audio")
        XCTAssertGreaterThanOrEqual(needle.peakL, needle.l, "peak hangs above the needle")
        let fallFrom = needle.l, fall = Date().addingTimeInterval(1)
        while Date() < fall { needle.step(meter); RunLoop.current.run(until: Date().addingTimeInterval(0.02)) }
        XCTAssertLessThan(needle.l, fallFrom, "must fall when audio stops")
    }

    func testMeterHistoryIsNewestLast() {
        let m = Meter()
        m.feedDemo()
        XCTAssertEqual(m.recent(100).count, 1)
        XCTAssertGreaterThan(m.read().peakL, 0)
    }
}
