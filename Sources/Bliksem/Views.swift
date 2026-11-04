import SwiftUI
import UniformTypeIdentifiers

enum Phosphor {
    static let amber = Color(red: 1.00, green: 0.66, blue: 0.12)
    static let bright = Color(red: 1.00, green: 0.82, blue: 0.35)
    static let dim = Color(red: 0.60, green: 0.38, blue: 0.08)
    static let bg = Color(red: 0.07, green: 0.045, blue: 0.03)
    static let fontName = "Px437 IBM VGA 9x16"
    /// Zoom steps that keep every font pixel a whole number of device pixels on Retina.
    static let zooms: [CGFloat] = [1, 1.5, 2, 2.5, 3]
    /// SwiftPM's Bundle.module looks beside the executable or in the build dir; the app keeps it in Contents/Resources.
    static let resources = Bundle.main.url(forResource: "Bliksem_Bliksem", withExtension: "bundle").flatMap(Bundle.init(url:)) ?? Bundle.module
    /// build.sh compiles Shaders/crt.metal into default.metallib inside that bundle.
    static let shaders = resources.url(forResource: "default", withExtension: "metallib").map(ShaderLibrary.init(url:)) ?? .bundle(resources)
}

/// The character cell, scaled by zoom.
struct Grid {
    var z: CGFloat = 1
    var w: CGFloat { 9 * z }
    var h: CGFloat { 16 * z }
    var font: Font { .custom(Phosphor.fontName, fixedSize: 16 * z) }
    var big: Font { .custom(Phosphor.fontName, fixedSize: 48 * z) }
}

private struct GridKey: EnvironmentKey { static let defaultValue = Grid() }
extension EnvironmentValues { var grid: Grid { get { self[GridKey.self] } set { self[GridKey.self] = newValue } } }

/// Fit a string to exactly n cells.
func pad(_ s: String, _ n: Int) -> String {
    let t = String(s.prefix(max(n, 0)))
    return t + String(repeating: " ", count: max(n - t.count, 0))
}

func hm(_ s: Int) -> String { s < 60 ? "" : s < 3600 ? "\(s / 60)m" : "\(s / 3600)h\(s % 3600 / 60)m" }

private struct Row: ViewModifier {
    @Environment(\.grid) private var g
    func body(content: Content) -> some View { content.frame(maxWidth: .infinity, alignment: .leading).frame(height: g.h) }
}

extension View {
    /// One text row on the grid.
    func row() -> some View { modifier(Row()) }
    func hand() -> some View { onHover { $0 ? NSCursor.pointingHand.push() : NSCursor.pop() } }

    func crt(zoom: CGFloat) -> some View {
        visualEffect { content, proxy in
            content
                .layerEffect(Phosphor.shaders.phosphor(.float2(proxy.size), .float(zoom)), maxSampleOffset: CGSize(width: 4 * zoom, height: 4 * zoom))
                .distortionEffect(Phosphor.shaders.curve(.float2(proxy.size), .float(0.045)), maxSampleOffset: CGSize(width: 80, height: 60))
        }
    }
}

struct ContentView: View {
    let m: Model
    @State private var booted = false
    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            Screen(m: m)
                .overlay { if !booted { Boot() } }
                .clipShape(RoundedRectangle(cornerRadius: 14))
                .padding(.top, 30)
                .padding([.horizontal, .bottom], 18)
                .crt(zoom: m.zoom)
        }
        .preferredColorScheme(.dark)
        .tint(Phosphor.amber)
        .environment(\.grid, Grid(z: m.zoom))
        .task {
            try? await Task.sleep(for: .seconds(2.6))
            withAnimation(.easeOut(duration: 0.9)) { booted = true }
        }
    }
}

/// The bolt and the name. The boot screen types it in; the UI keeps a faint burn-in of it.
struct Logo: View {
    @Environment(\.grid) private var g
    var shown = Int.max
    static let bolt = [
        "......XXXXXX", ".....XXXXXX.", "....XXXXXX..", "...XXXXXX...", "..XXXXXXXXXX", ".XXXXXXXXXX.",
        "......XXXX..", ".....XXXX...", "....XXXX....", "...XXXX.....", "..XXX.......", ".XX.........", "X...........",
    ].map { String($0.map { $0 == "X" ? "█" : " " }) }
    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(Self.bolt.prefix(shown).enumerated()), id: \.offset) { _, r in Text(r).frame(height: g.h) }
            if shown > Self.bolt.count { Text(Copy.app).font(g.big).frame(height: 3 * g.h).padding(.top, g.h) }
            if shown > Self.bolt.count + 1 { Text(Copy.boot).foregroundStyle(Phosphor.dim).frame(height: g.h) }
        }
    }
}

struct Boot: View {
    @Environment(\.grid) private var g
    @State private var start = Date()
    var body: some View {
        TimelineView(.periodic(from: start, by: 0.08)) { ctx in
            let shown = Int(ctx.date.timeIntervalSince(start) / 0.08)
            ZStack {
                Phosphor.bg
                VStack(spacing: 0) {
                    Logo(shown: shown)
                    Text(shown > Logo.bolt.count + 1 && shown % 6 < 3 ? "█" : " ").frame(height: g.h).padding(.top, g.h)
                }
            }
        }
    }
}

struct Screen: View {
    let m: Model
    @Environment(\.grid) private var g
    var body: some View {
        GeometryReader { geo in
            let cols = Int(geo.size.width / g.w)
            ZStack {
                Logo().opacity(0.07).blur(radius: 0.4 * g.z)
                VStack(spacing: 0) {
                    HStack(spacing: 0) {
                        Box(title: Copy.app) { c, _ in ChannelPane(m: m, cols: c) }.frame(width: CGFloat(min(36, max(22, cols * 2 / 5))) * g.w)
                        Box(title: Copy.now) { c, _ in NowPane(m: m, cols: c) }
                    }
                    StatusBar(m: m, cols: cols)
                }
            }
        }
        .font(g.font)
        .foregroundStyle(Phosphor.amber)
        .background(Phosphor.bg)
    }
}

/// Double-line DOS box drawn with real box characters; content gets its inner size in cells.
struct Box<Content: View>: View {
    let title: String
    @ViewBuilder let content: (Int, Int) -> Content
    @Environment(\.grid) private var g
    var body: some View {
        GeometryReader { geo in
            let cols = Int(geo.size.width / g.w)
            let rows = max(Int(geo.size.height / g.h) - 2, 0)
            let inner = max(cols - 2, 0)
            VStack(alignment: .leading, spacing: 0) {
                Text("╔═[ \(title) ]" + String(repeating: "═", count: max(inner - title.count - 5, 0)) + "╗").row()
                HStack(spacing: 0) {
                    side(rows)
                    content(inner, rows)
                        .frame(width: CGFloat(inner) * g.w, height: CGFloat(rows) * g.h, alignment: .topLeading)
                        .clipped()
                    side(rows)
                }
                Text("╚" + String(repeating: "═", count: inner) + "╝").row()
            }
        }
    }
    private func side(_ rows: Int) -> some View {
        VStack(spacing: 0) { ForEach(0..<rows, id: \.self) { _ in Text("║").frame(height: g.h) } }
    }
}

struct Btn: View {
    let label: String
    var on = false
    var color = Phosphor.amber
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            Text(label).foregroundStyle(on ? Phosphor.bg : color).background(on ? Phosphor.amber : .clear)
        }.buttonStyle(.plain).hand()
    }
}

/// Text input on the character grid with a blinking block cursor. Typing, backspace, enter submits,
/// escape clears then unfocuses, paste works, [x] clears, optional [ OK ].
struct GridInput: View {
    let prompt: String
    @Binding var text: String
    var secret = false
    var placeholder = ""
    var focusTrigger = 0
    var submitLabel: String? = nil
    var onSubmit: () -> Void = {}
    @FocusState private var focused: Bool
    var body: some View {
        let shown = secret ? String(repeating: "*", count: min(text.count, 8)) : text
        HStack(spacing: 0) {
            Text(prompt).foregroundStyle(focused ? Phosphor.bg : Phosphor.amber).background(focused ? Phosphor.amber : .clear)
            if shown.isEmpty && !focused {
                Text(placeholder).foregroundStyle(Phosphor.dim)
            } else {
                Text(shown).foregroundStyle(Phosphor.bright)
            }
            if focused {
                TimelineView(.periodic(from: .now, by: 0.5)) { ctx in
                    Text(Int(ctx.date.timeIntervalSinceReferenceDate * 2) % 2 == 0 ? "█" : " ")
                }
            }
            Spacer(minLength: 0)
            if !text.isEmpty {
                if let submitLabel { Btn(label: submitLabel) { submit() } } else { Btn(label: "[x]") { text = "" } }
            }
        }
        .contentShape(Rectangle())
        .focusable(true, interactions: .edit).focusEffectDisabled()
        .focused($focused)
        .onTapGesture { focused = true }
        .onChange(of: focusTrigger) { focused = true }
        .onKeyPress(phases: .down) { p in
            if p.modifiers.contains(.command) || p.modifiers.contains(.control) { return .ignored }
            if p.key == .delete || p.characters == "\u{7F}" || p.characters == "\u{8}" {
                if !text.isEmpty { text.removeLast() }
                return .handled
            }
            if p.key == .return { submit(); return .handled }
            if p.key == .escape {
                if text.isEmpty { focused = false } else { text = "" }
                return .handled
            }
            guard p.characters.count == 1, let c = p.characters.first,
                  c == " " || c.isLetter || c.isNumber || c.isPunctuation || c.isSymbol else { return .ignored }
            text.append(c)
            return .handled
        }
        .onPasteCommand(of: [.text]) { providers in
            providers.first?.loadObject(ofClass: NSString.self) { s, _ in
                guard let s = s as? String else { return }
                DispatchQueue.main.async { text += s.trimmingCharacters(in: .whitespacesAndNewlines) }
            }
        }
    }
    private func submit() { focused = false; onSubmit() }
}

struct ChannelPane: View {
    @Bindable var m: Model
    let cols: Int
    var body: some View {
        VStack(spacing: 0) {
            GridInput(prompt: Copy.find, text: $m.query, placeholder: Copy.search, focusTrigger: m.focusSearch) {
                if let c = m.visible.first { m.play(c) }
            }.row()
            HStack(spacing: 0) {
                Text(" ")
                Btn(label: "[ \(Copy.techno) ]", on: !m.showAll) { m.showAll = false }
                Text(" ")
                Btn(label: "[ \(cols < 28 ? Copy.allShort : Copy.all) ]", on: m.showAll) { m.showAll = true }
            }.row()
            Text(String(repeating: "─", count: cols)).row()
            ScrollView(showsIndicators: false) {
                VStack(spacing: 0) { ForEach(m.visible) { c in ChannelRows(c: c, m: m, cols: cols) } }
            }
            .focusable().focusEffectDisabled()
            .onKeyPress(.space) { m.toggle(); return .handled }
            .onKeyPress(.upArrow) { m.hop(-1); return .handled }
            .onKeyPress(.downArrow) { m.hop(1); return .handled }
            Text(String(repeating: "─", count: cols)).row()
            GridInput(prompt: Copy.keyPrompt, text: $m.listenKey, secret: true, placeholder: Copy.keyPlaceholder, submitLabel: Copy.ok) {
                if let c = m.current ?? m.visible.first, !m.playing { m.play(c) }
            }.row()
            Text(pad(Copy.keyHint, cols)).foregroundStyle(Phosphor.dim).row()
        }
    }
}

struct ChannelRows: View {
    let c: Channel
    let m: Model
    let cols: Int
    var body: some View {
        let on = m.current?.id == c.id
        let time = hm(m.seconds[c.key] ?? 0)
        let np = m.onAir[c.id].map { "  \($0.artist) - \($0.title)" } ?? ""
        VStack(spacing: 0) {
            Text(pad(" " + c.name.uppercased(), cols - time.count - 1) + time + " ").row()
            Text(pad(np, cols)).foregroundStyle(on ? Phosphor.bg : Phosphor.dim).row()
        }
        .foregroundStyle(on ? Phosphor.bg : Phosphor.amber)
        .background(on ? Phosphor.amber : .clear)
        .contentShape(Rectangle())
        .onTapGesture { m.play(c) }
        .hand()
    }
}

struct Art: View {
    let url: URL?
    let w: CGFloat
    let h: CGFloat
    var body: some View {
        AsyncImage(url: url) { $0.resizable().interpolation(.none).scaledToFill() } placeholder: { Phosphor.dim.opacity(0.25) }
            .frame(width: w, height: h).clipped()
    }
}

struct NowPane: View {
    let m: Model
    let cols: Int
    @Environment(\.grid) private var g
    @State private var tab = 0
    @State private var heardQuery = ""
    var body: some View {
        let artCols = 21
        let w = cols - artCols - 1
        VStack(alignment: .leading, spacing: 0) {
            if let c = m.current {
                HStack(alignment: .top, spacing: 0) {
                    Art(url: img(m.nowArtPath, 96) ?? img(c.asset_url, 96), w: CGFloat(artCols) * g.w, h: 12 * g.h)
                    Text(" ")
                    VStack(alignment: .leading, spacing: 0) {
                        Text(pad(" DI.FM " + c.name.uppercased(), w)).foregroundStyle(Phosphor.bright).row()
                        Text("").row()
                        Text(" " + (m.nowTrack?.title ?? Copy.nothing)).foregroundStyle(Phosphor.bright).lineLimit(3)
                            .frame(width: CGFloat(max(w, 1)) * g.w, height: 3 * g.h, alignment: .topLeading)
                        Text(pad(" " + (m.nowTrack?.artist ?? ""), w)).row()
                        Text("").row()
                        Text(pad(" " + votes + listened(c), w)).foregroundStyle(Phosphor.dim).row()
                        Text("").row()
                        HStack(spacing: 0) {
                            Text(" ")
                            Btn(label: m.playing ? Copy.stop : Copy.play) { m.toggle() }
                            Text("  ")
                            if let t = m.nowTrack {
                                Btn(label: m.isLiked(t.id) ? Copy.liked : Copy.like, on: m.isLiked(t.id)) { m.likeCurrent() }
                            }
                        }.row()
                        Text("").row()
                        statusRow
                    }
                }.frame(height: 12 * g.h)
            } else {
                Text(pad(" " + Copy.idle, cols)).foregroundStyle(Phosphor.bright).row()
                    .frame(height: 12 * g.h, alignment: .top)
            }
            Text("").row()
            HStack(spacing: 0) {
                Text(" ")
                let narrow = cols < 70
                Btn(label: "[ \(narrow ? Copy.lastOnShort : Copy.lastOn) ]", on: tab == 0) { tab = 0 }
                Text(" ")
                Btn(label: "[ \(narrow ? Copy.likesShort : Copy.likes) ]", on: tab == 1) { tab = 1 }
                Text(" ")
                Btn(label: "[ \(narrow ? Copy.heardShort : Copy.heard) ]", on: tab == 2) { tab = 2 }
            }.row()
            Text(String(repeating: "─", count: cols)).row()
            switch tab {
            case 0: HistoryList(m: m, cols: cols)
            case 1: LikesList(m: m, cols: cols)
            default: HeardList(m: m, cols: cols, query: $heardQuery)
            }
        }
    }

    private var votes: String {
        guard let t = m.nowTrack, let v = m.history.first(where: { $0.track_id == t.id })?.votes else { return "" }
        return "▲\(v.up) ▼\(v.down)   "
    }

    @ViewBuilder private var statusRow: some View {
        if m.status == Copy.loading {
            TimelineView(.periodic(from: .now, by: 0.1)) { ctx in
                let n = Int(ctx.date.timeIntervalSinceReferenceDate * 10) % 21
                Text(" [" + String(repeating: "█", count: n) + String(repeating: "░", count: 20 - n) + "] " + Copy.loading).row()
            }
        } else {
            Text(pad(" " + m.status, w)).foregroundStyle(Phosphor.bright).row()
        }
    }

    private var w: Int { cols - 22 }

    private func listened(_ c: Channel) -> String {
        let t = hm(m.seconds[c.key] ?? 0)
        return t.isEmpty ? "" : t + " " + Copy.onChannel
    }
}

struct Check: View {
    let on: Bool
    var body: some View { Text(on ? "[*]" : "[ ]").foregroundStyle(on ? Phosphor.bright : Phosphor.amber) }
}

struct HistoryList: View {
    let m: Model
    let cols: Int
    var body: some View {
        ScrollView(showsIndicators: false) {
            LazyVStack(spacing: 0) {
                ForEach(m.history) { t in
                    let time = t.started.map { Date(timeIntervalSince1970: $0).formatted(.dateTime.hour().minute()) } ?? "--:--"
                    let right = (t.votes.map { " ▲\($0.up) ▼\($0.down) " } ?? " ") + "    "
                    HStack(spacing: 0) {
                        Text(pad(" \(time)  \(t.artist) - \(t.title)", cols - right.count) + right)
                        Check(on: m.isLiked(t.track_id))
                        Text(" ")
                    }
                    .row().contentShape(Rectangle()).hand()
                    .onTapGesture { m.toggleLike(id: t.track_id, artist: t.artist, title: t.title, channel: m.current?.name ?? "", art: t.art_url) }
                }
                if m.history.isEmpty { Text(pad(" " + (m.current == nil ? Copy.idle : Copy.loading), cols)).foregroundStyle(Phosphor.dim).row() }
            }
        }
    }
}

struct LikesList: View {
    let m: Model
    let cols: Int
    var body: some View {
        ScrollView(showsIndicators: false) {
            LazyVStack(spacing: 0) {
                ForEach(m.likes) { l in
                    let apple = l.appleState == "found" ? Copy.onApple : l.appleState == "none" ? Copy.notOnApple : Copy.appleSearching
                    let tail = apple.count + 22
                    HStack(spacing: 0) {
                        Text(pad(" \(l.date.formatted(.dateTime.day().month())) \(l.artist) - \(l.title)  ", cols - tail))
                        Btn(label: apple, color: l.appleState == "found" ? Phosphor.bright : Phosphor.dim) {
                            NSWorkspace.shared.open(l.apple.flatMap(URL.init) ?? AppleMusic.searchURL("\(l.artist) \(l.title)"))
                        }
                        Text(" ")
                        ForEach(Links.all(for: "\(l.artist) \(l.title)"), id: \.0) { name, url in
                            Btn(label: "[\(name.prefix(2).uppercased())]") { NSWorkspace.shared.open(url) }
                        }
                        Text(" ")
                        Btn(label: "[x]") { m.toggleLike(id: l.id, artist: l.artist, title: l.title, channel: l.channel, art: l.art) }
                    }.row()
                }
                if m.likes.isEmpty { Text(pad(" " + Copy.noLikes, cols)).foregroundStyle(Phosphor.dim).row() }
            }
        }
    }
}

struct HeardList: View {
    let m: Model
    let cols: Int
    @Binding var query: String
    var body: some View {
        let q = query.lowercased()
        let rows = m.heard.reversed().filter { q.isEmpty || "\($0.artist) \($0.title) \($0.channel)".lowercased().contains(q) }
        VStack(spacing: 0) {
            GridInput(prompt: Copy.find, text: $query, placeholder: Copy.heardSearch).row()
            ScrollView(showsIndicators: false) {
                LazyVStack(spacing: 0) {
                    ForEach(rows) { h in
                        HStack(spacing: 0) {
                            Text(pad(" \(h.date.formatted(.dateTime.day().month().hour().minute()))  \(h.channel.uppercased())  \(h.artist) - \(h.title)", cols - 5))
                            Text(" ")
                            Check(on: m.isLiked(h.trackID))
                            Text(" ")
                        }
                        .row().contentShape(Rectangle()).hand()
                        .onTapGesture { m.toggleLike(id: h.trackID, artist: h.artist, title: h.title, channel: h.channel, art: h.art) }
                    }
                    if rows.isEmpty { Text(pad(" " + Copy.noHeard, cols)).foregroundStyle(Phosphor.dim).row() }
                }
            }
        }
    }
}

struct StatusBar: View {
    let m: Model
    let cols: Int
    var body: some View {
        let state = m.current.map { "\(m.playing ? "playing" : "stopped") \($0.key)" } ?? "idle"
        let left = " \(state) │ \(m.key.isEmpty ? Copy.noKeyShort : Copy.premium) │ \(m.likes.count) liked"
        let legend = cols >= left.count + Copy.legend.count + 8 ? Copy.legend : ""
        HStack(spacing: 0) {
            Text(pad(left, cols - legend.count - 8) + legend)
            Btn(label: "[-]", color: Phosphor.bg) { m.zoomOut() }
            Text(" ")
            Btn(label: "[+]", color: Phosphor.bg) { m.zoomIn() }
            Text(" ")
        }
        .foregroundStyle(Phosphor.bg).background(Phosphor.amber).row()
    }
}
