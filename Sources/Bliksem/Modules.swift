import SwiftUI

/// The panels that can sit in the bay under Now Playing. Add a case here and a view in ModuleView.
enum Module: String, CaseIterable, Codable, Identifiable {
    case vu, wave, last, liked, heard, habits
    var id: String { rawValue }
    var title: String {
        switch self {
        case .vu: Copy.vu
        case .wave: Copy.wave
        case .last: Copy.lastOn
        case .liked: Copy.likes
        case .heard: Copy.heard
        case .habits: Copy.habits
        }
    }
    var short: String {
        switch self {
        case .vu: Copy.vuShort
        case .wave: Copy.wave
        case .last: Copy.lastOnShort
        case .liked: Copy.likesShort
        case .heard: Copy.heardShort
        case .habits: Copy.habits
        }
    }
    /// Meters stay narrow; lists share what is left.
    var fixedCols: Int? { self == .vu ? 20 : nil }
    var minCols: Int { fixedCols ?? 28 }
}

/// Which slots fit in `cols` and how wide each is, in whole cells so the grid stays true.
func fitModules(_ slots: [Module], cols: Int) -> [(module: Module, cols: Int)] {
    var shown: [Module] = []
    var used = 0
    for s in slots where used + s.minCols <= cols { shown.append(s); used += s.minCols }
    if shown.isEmpty, let first = slots.first { shown = [first] }
    let fixed = shown.compactMap(\.fixedCols).reduce(0, +)
    let flex = shown.filter { $0.fixedCols == nil }.count
    let each = flex > 0 ? (cols - fixed) / flex : 0
    var rest = cols - fixed - each * flex
    if flex == 0 { rest = cols - fixed }
    var lastIndex = shown.lastIndex { $0.fixedCols == nil } ?? shown.count - 1
    if shown.isEmpty { lastIndex = 0 }
    return shown.enumerated().map { i, m in (m, (m.fixedCols ?? each) + (i == lastIndex ? rest : 0)) }
}

/// Linear amplitude to 0...1 on the meters' -36...0 dBFS scale.
func meterLevel(_ x: Float) -> Float {
    guard x > 0 else { return 0 }
    return min(max((20 * log10(x) + 36) / 36, 0), 1)
}

/// Linear amplitude to 0...1 for the waveform. Measured on DI Techno: short-term loudness sits between about -18 and -6 dBFS,
/// so a -36 dB floor would draw a flat bar. Squared so kicks stand out.
func waveLevel(_ x: Float) -> Float {
    guard x > 0 else { return 0 }
    let n = min(max((20 * log10(x) + 24) / 19, 0), 1)
    return n * n
}

/// A rolling loudness waveform as text: one column per level, newest right, mirrored around the centre, two half-blocks per row.
func waveRows(_ levels: [Float], cols: Int, rows: Int) -> [String] {
    guard cols > 0, rows > 0 else { return [] }
    let half = rows * 2
    let heights = (Array(repeating: 0, count: max(cols - levels.count, 0)) + levels.suffix(cols)).map { waveLevel($0) * Float(half) }
    return (0..<rows).map { r in
        String(heights.map { h -> Character in
            let top = abs(Float(2 * r) + 0.5 - Float(half) / 2) < h / 2
            let bottom = abs(Float(2 * r + 1) + 0.5 - Float(half) / 2) < h / 2
            return top && bottom ? "█" : top ? "▀" : bottom ? "▄" : " "
        })
    }
}

/// Needle physics: fast up, slow down, a peak that hangs. Only the meters read it.
final class Ballistics {
    var l: Float = 0, r: Float = 0, peakL: Float = 0, peakR: Float = 0
    private var at = Date(), heldL = Date.distantPast, heldR = Date.distantPast

    func step(_ m: Meter) {
        let v = m.read(), now = Date(), dt = Float(now.timeIntervalSince(at))
        at = now
        l = approach(l, meterLevel(v.rmsL), dt)
        r = approach(r, meterLevel(v.rmsR), dt)
        hold(&peakL, meterLevel(v.peakL), &heldL, now, dt)
        hold(&peakR, meterLevel(v.peakR), &heldR, now, dt)
    }
    private func approach(_ a: Float, _ b: Float, _ dt: Float) -> Float {
        a + (b - a) * (1 - exp(-dt / (b > a ? 0.05 : 0.3)))
    }
    private func hold(_ p: inout Float, _ v: Float, _ held: inout Date, _ now: Date, _ dt: Float) {
        if v >= p { p = v; held = now } else if now.timeIntervalSince(held) > 1.2 { p = max(v, p - dt * 0.4) }
    }
}

struct VUModule: View {
    let m: Model
    let cols: Int
    let rows: Int
    var body: some View {
        TimelineView(.animation(minimumInterval: m.playing ? 0.05 : 0.5)) { _ in
            let _ = m.needle.step(m.radio.meter)
            // Plain values, not the object: SwiftUI skips the rows below if what they capture looks unchanged.
            let l = m.needle.l, r = m.needle.r, pl = m.needle.peakL, pr = m.needle.peakR
            let bars = max(rows - 1, 1), w = max((cols - 7) / 2, 1)
            VStack(alignment: .leading, spacing: 0) {
                ForEach(0..<bars, id: \.self) { i in
                    let at = Float(bars - i) / Float(bars)   // top of this row on the 0...1 scale
                    let db = Int(((at - 1) * 36).rounded())
                    HStack(spacing: 0) {
                        Text(pad(i % 3 == 0 ? String(format: "%4d", db) : "", 5)).foregroundStyle(Phosphor.dim)
                        cell(l, pl, at, bars, w, db)
                        Text(" ")
                        cell(r, pr, at, bars, w, db)
                    }.row()
                }
                HStack(spacing: 0) {
                    Text("  dB ").foregroundStyle(Phosphor.dim)
                    Text(pad(" L", w + 1)); Text(pad(" R", w))
                }.row()
            }
        }
    }

    private func cell(_ level: Float, _ peak: Float, _ at: Float, _ bars: Int, _ w: Int, _ db: Int) -> some View {
        let step = 1 / Float(bars)
        let lit = level >= at - step / 2
        let isPeak = peak >= at - step && peak < at + step * 0.01
        let hot = db > -6
        return Text(String(repeating: lit || isPeak ? "█" : "░", count: w))
            .foregroundStyle(lit || isPeak ? (hot ? Phosphor.bright : Phosphor.amber) : Phosphor.dim.opacity(0.35))
    }
}

struct WaveModule: View {
    let m: Model
    let cols: Int
    let rows: Int
    var body: some View {
        TimelineView(.animation(minimumInterval: m.playing ? 0.05 : 0.5)) { _ in
            let lines = waveRows(m.radio.meter.recent(cols), cols: cols, rows: rows)
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(lines.enumerated()), id: \.offset) { _, l in Text(l).row() }
            }
        }
    }
}

struct HabitsModule: View {
    let m: Model
    let cols: Int
    var body: some View {
        let ranked = m.channels.filter { (m.seconds[$0.key] ?? 0) >= 60 }.sorted { m.seconds[$0.key]! > m.seconds[$1.key]! }
        let total = m.seconds.values.reduce(0, +), most = ranked.first.flatMap { m.seconds[$0.key] } ?? 1
        let nameW = min(max(cols / 2, 10), 22), barW = max(cols - nameW - 9, 1)
        Scroller {
            Text(pad(" \(hm(total).isEmpty ? "0m" : hm(total)) \(Copy.total) · \(m.heard.count) \(Copy.tracks) · \(m.likes.count) \(Copy.likedWord)", cols)).foregroundStyle(Phosphor.bright).row()
            Text("").row()
            ForEach(ranked) { c in
                let s = m.seconds[c.key] ?? 0
                let n = max(s * barW / max(most, 1), 1)
                HStack(spacing: 0) {
                    Text(pad(" " + c.name.uppercased(), nameW))
                    Text(String(repeating: "█", count: n))
                    Text(String(repeating: "░", count: barW - n)).foregroundStyle(Phosphor.dim.opacity(0.35))
                    Text(pad(" " + hm(s), 8))
                }.row().contentShape(Rectangle()).hand().onTapGesture { m.play(c) }
            }
            if ranked.isEmpty { Text(pad(" " + Copy.noHabits, cols)).foregroundStyle(Phosphor.dim).row() }
        }
    }
}
