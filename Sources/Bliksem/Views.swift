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
private struct SnapshotKey: EnvironmentKey { static let defaultValue = false }
private struct SnapArtKey: EnvironmentKey { static let defaultValue: NSImage? = nil }
extension EnvironmentValues {
    var grid: Grid { get { self[GridKey.self] } set { self[GridKey.self] = newValue } }
    /// Offscreen renders skip ScrollView content and async images, so headless snapshots swap them.
    var snapshot: Bool { get { self[SnapshotKey.self] } set { self[SnapshotKey.self] = newValue } }
    var snapArt: NSImage? { get { self[SnapArtKey.self] } set { self[SnapArtKey.self] = newValue } }
}

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

/// A scrolling list, or a plain stack in headless snapshots.
struct Scroller<Content: View>: View {
    @ViewBuilder let content: Content
    @Environment(\.snapshot) private var snapshot
    var body: some View {
        if snapshot {
            VStack(spacing: 0) { content }.frame(maxHeight: .infinity, alignment: .top)
        } else {
            ScrollView(showsIndicators: false) { LazyVStack(spacing: 0) { content } }
        }
    }
}

struct ContentView: View {
    let m: Model
    @State private var booted = false
    @FocusState private var root: Bool
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
        .focusable().focusEffectDisabled().focused($root)
        .onKeyPress(.space) { m.toggle(); return .handled }
        .onKeyPress(.upArrow) { m.hop(-1); return .handled }
        .onKeyPress(.downArrow) { m.hop(1); return .handled }
        .onChange(of: m.focusRoot) { root = true }
        .preferredColorScheme(.dark)
        .tint(Phosphor.amber)
        .environment(\.grid, Grid(z: m.zoom))
        .task {
            root = true
            try? await Task.sleep(for: .seconds(2.6))
            withAnimation(.easeOut(duration: 0.9)) { booted = true }
        }
    }
}

/// The bolt and the name, typed in on the boot screen.
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
    /// Art plus its info block, in rows, inside the box.
    static let nowRows = 12
    var body: some View {
        GeometryReader { geo in
            let cols = Int(geo.size.width / g.w), rows = Int(geo.size.height / g.h)
            let side = min(36, max(24, cols * 2 / 5)), right = max(cols - side, 1)
            let nowBox = Self.nowRows + 2, bay = max(rows - 1 - nowBox, 0)
            VStack(alignment: .leading, spacing: 0) {
                HStack(alignment: .top, spacing: 0) {
                    Box(title: Copy.app) { c, r in Sidebar(m: m, cols: c, rows: r) }
                        .frame(width: CGFloat(side) * g.w, height: CGFloat(rows - 1) * g.h)
                    VStack(alignment: .leading, spacing: 0) {
                        Box(title: Copy.now) { c, _ in NowPane(m: m, cols: c) }
                            .frame(width: CGFloat(right) * g.w, height: CGFloat(nowBox) * g.h)
                        if bay >= 4 { Bay(m: m, cols: right, rows: bay) }
                    }
                }
                StatusBar(m: m, cols: cols)
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
    var head: ((Int) -> AnyView)? = nil
    @ViewBuilder let content: (Int, Int) -> Content
    @Environment(\.grid) private var g
    var body: some View {
        GeometryReader { geo in
            let cols = Int(geo.size.width / g.w)
            let rows = max(Int(geo.size.height / g.h) - 2, 0)
            let inner = max(cols - 2, 0)
            VStack(alignment: .leading, spacing: 0) {
                if let head {
                    head(inner).row()
                } else {
                    Text("╔═[ \(title) ]" + String(repeating: "═", count: max(inner - title.count - 5, 0)) + "╗").row()
                }
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
    var onLeave: () -> Void = {}
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
                if text.isEmpty { focused = false; onLeave() } else { text = "" }
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
    private func submit() { focused = false; onLeave(); onSubmit() }
}

struct Sidebar: View {
    @Bindable var m: Model
    let cols: Int
    let rows: Int
    var body: some View {
        VStack(spacing: 0) {
            GridInput(prompt: Copy.find, text: $m.query, placeholder: Copy.search, focusTrigger: m.focusSearch, onLeave: { m.focusRoot += 1 }) {
                if let c = m.visible.first { m.play(c) }
            }.row()
            Text(String(repeating: "─", count: cols)).row()
            Scroller {
                ForEach(m.sections) { s in
                    SectionHead(s: s, m: m, cols: cols)
                    if !m.collapsed.contains(s.id) {
                        ForEach(s.channels) { c in
                            ChannelRow(c: c, m: m, cols: cols, live: s.id == Model.favSection)
                        }
                        if s.id == Model.favSection && s.channels.isEmpty {
                            Text(pad(Copy.noFavs, cols)).foregroundStyle(Phosphor.dim).row()
                        }
                    }
                }
            }
            Text(String(repeating: "─", count: cols)).row()
            GridInput(prompt: Copy.keyPrompt, text: $m.listenKey, secret: true, placeholder: Copy.keyPlaceholder, submitLabel: Copy.ok, onLeave: { m.focusRoot += 1 }) {
                if let c = m.current ?? m.visible.first, !m.playing { m.play(c) }
            }.row()
            if m.key.isEmpty { Text(pad(Copy.keyHint, cols)).foregroundStyle(Phosphor.dim).row() }
        }
    }
}

struct SectionHead: View {
    let s: ChannelSection
    let m: Model
    let cols: Int
    var body: some View {
        let closed = m.collapsed.contains(s.id)
        Text(pad(" \(closed ? "►" : "▼") \(s.title) (\(s.channels.count))", cols))
            .foregroundStyle(Phosphor.bright).row()
            .contentShape(Rectangle()).hand()
            .onTapGesture { m.toggleCollapsed(s.id) }
    }
}

struct ChannelRow: View {
    let c: Channel
    let m: Model
    let cols: Int
    /// Favourites and the channel you're on show what's playing; the rest stay one line so 119 channels stay scannable.
    let live: Bool
    var body: some View {
        let on = m.current?.id == c.id
        let time = hm(m.seconds[c.key] ?? 0)
        let ink = on ? Phosphor.bg : Phosphor.amber
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                Btn(label: m.isFav(c.id) ? "[*]" : "[ ]", color: on ? Phosphor.bg : m.isFav(c.id) ? Phosphor.bright : Phosphor.dim) { m.toggleFav(c.id) }
                Text(pad(" " + c.name.uppercased(), cols - 3 - time.count - 1) + time + " ").foregroundStyle(ink)
                    .contentShape(Rectangle()).onTapGesture { m.play(c) }.hand()
            }.row()
            if live || on, let t = m.onAir[c.id] {
                Text(pad("    \(t.artist) - \(t.title)", cols)).foregroundStyle(on ? Phosphor.bg : Phosphor.dim).row()
                    .contentShape(Rectangle()).onTapGesture { m.play(c) }.hand()
            }
        }
        .background(on ? Phosphor.amber : .clear)
    }
}

/// Album art with the colour burned out: one phosphor, chunky pixels.
struct Art: View {
    let url: URL?
    let w: CGFloat
    let h: CGFloat
    @Environment(\.snapArt) private var snapArt
    var body: some View {
        Group {
            if let snapArt {
                phosphor(Image(nsImage: snapArt))
            } else {
                AsyncImage(url: url) { phosphor($0) } placeholder: { Phosphor.dim.opacity(0.25) }
            }
        }
        .frame(width: w, height: h).clipped()
    }
    private func phosphor(_ i: Image) -> some View {
        i.resizable().interpolation(.none).scaledToFill().grayscale(1).contrast(1.2).colorMultiply(Phosphor.amber)
    }
}

struct NowPane: View {
    let m: Model
    let cols: Int
    @Environment(\.grid) private var g
    var body: some View {
        let artCols = 21
        let w = cols - artCols - 1
        if let c = m.current {
            HStack(alignment: .top, spacing: 0) {
                Art(url: img(m.nowArtPath, 96) ?? img(c.asset_url, 96), w: CGFloat(artCols) * g.w, h: CGFloat(Screen.nowRows) * g.h)
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
                    statusRow(w)
                }
            }
        } else {
            Text(pad(" " + Copy.idle, cols)).foregroundStyle(Phosphor.bright).row()
        }
    }

    private var votes: String {
        guard let t = m.nowTrack, let v = m.history.first(where: { $0.track_id == t.id })?.votes else { return "" }
        return "▲\(v.up) ▼\(v.down)   "
    }

    @ViewBuilder private func statusRow(_ w: Int) -> some View {
        if m.status == Copy.loading {
            TimelineView(.periodic(from: .now, by: 0.1)) { ctx in
                let n = Int(ctx.date.timeIntervalSinceReferenceDate * 10) % 21
                Text(" [" + String(repeating: "█", count: n) + String(repeating: "░", count: 20 - n) + "] " + Copy.loading).row()
            }
        } else {
            Text(pad(" " + m.status, w)).foregroundStyle(Phosphor.bright).row()
        }
    }

    private func listened(_ c: Channel) -> String {
        let t = hm(m.seconds[c.key] ?? 0)
        return t.isEmpty ? "" : t + " " + Copy.onChannel
    }
}

/// The modular area: as many boxes side by side as fit. Each box flips through modules with [<] [>].
struct Bay: View {
    @Environment(\.grid) private var g
    let m: Model
    let cols: Int
    let rows: Int
    var body: some View {
        let fit = fitModules(m.slots, cols: cols)
        let used = fit.map(\.module.minCols).reduce(0, +)
        let room = m.slots.count == fit.count && m.slots.count < Module.allCases.count && used + Module.last.minCols <= cols
        HStack(spacing: 0) {
            ForEach(Array(fit.enumerated()), id: \.element.module) { i, f in
                Box(title: f.module.title, head: { inner in
                    AnyView(head(f.module, slot: i, inner: inner, canAdd: room && i == fit.count - 1, canRemove: m.slots.count > 1))
                }) { c, r in
                    ModuleView(module: f.module, m: m, cols: c, rows: r)
                }
                .frame(width: CGFloat(f.cols) * g.w, height: CGFloat(rows) * g.h)
            }
        }
    }

    private func head(_ mod: Module, slot: Int, inner: Int, canAdd: Bool, canRemove: Bool) -> some View {
        let controls = 6 + (canAdd ? 3 : 0) + (canRemove ? 3 : 0)
        let title = mod.title.count + controls + 2 <= inner ? mod.title : mod.short
        return HStack(spacing: 0) {
            Text("╔")
            Btn(label: "[<]") { m.cycle(slot: slot, by: -1) }
            Text(" \(title) ").foregroundStyle(Phosphor.bright)
            Btn(label: "[>]") { m.cycle(slot: slot, by: 1) }
            Text(String(repeating: "═", count: max(inner - controls - title.count - 2, 0)))
            if canAdd { Btn(label: "[+]") { m.addSlot() } }
            if canRemove { Btn(label: "[x]") { m.removeSlot(slot) } }
            Text("╗")
        }
    }
}

struct ModuleView: View {
    let module: Module
    let m: Model
    let cols: Int
    let rows: Int
    @State private var heardQuery = ""
    var body: some View {
        switch module {
        case .vu: VUModule(m: m, cols: cols, rows: rows)
        case .wave: WaveModule(m: m, cols: cols, rows: rows)
        case .last: HistoryList(m: m, cols: cols)
        case .liked: LikesList(m: m, cols: cols)
        case .heard: HeardList(m: m, cols: cols, query: $heardQuery)
        case .habits: HabitsModule(m: m, cols: cols)
        }
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
        Scroller {
            ForEach(m.history) { t in
                let time = t.started.map { Date(timeIntervalSince1970: $0).formatted(.dateTime.hour().minute()) } ?? "--:--"
                let right = cols >= 56 ? (t.votes.map { " ▲\($0.up) ▼\($0.down) " } ?? " ") : " "
                HStack(spacing: 0) {
                    Text(pad(" \(time) \(t.artist) - \(t.title)", cols - right.count - 4) + right)
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

struct LikesList: View {
    let m: Model
    let cols: Int
    var body: some View {
        Scroller {
            ForEach(m.likes) { l in
                let found = l.appleState == "found"
                let wide = cols >= 72
                let apple = !wide ? "[AM]" : found ? Copy.onApple : l.appleState == "none" ? Copy.notOnApple : Copy.appleSearching
                let links = Links.all(for: "\(l.artist) \(l.title)")
                let tail = apple.count + 1 + (wide ? links.count * 4 + 1 : 0) + 4
                HStack(spacing: 0) {
                    Text(pad(" \(l.artist) - \(l.title) ", cols - tail))
                    Btn(label: apple, color: found ? Phosphor.bright : Phosphor.dim) {
                        NSWorkspace.shared.open(l.apple.flatMap(URL.init) ?? AppleMusic.searchURL("\(l.artist) \(l.title)"))
                    }
                    Text(" ")
                    if wide {
                        ForEach(links, id: \.0) { name, url in
                            Btn(label: "[\(name.prefix(2).uppercased())]") { NSWorkspace.shared.open(url) }
                        }
                        Text(" ")
                    }
                    Btn(label: "[x]") { m.toggleLike(id: l.id, artist: l.artist, title: l.title, channel: l.channel, art: l.art) }
                    Text(" ")
                }.row()
            }
            if m.likes.isEmpty { Text(pad(" " + Copy.noLikes, cols)).foregroundStyle(Phosphor.dim).row() }
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
            GridInput(prompt: Copy.find, text: $query, placeholder: Copy.heardSearch, onLeave: { m.focusRoot += 1 }).row()
            Scroller {
                ForEach(rows) { h in
                    HStack(spacing: 0) {
                        Text(pad(" \(h.date.formatted(.dateTime.day().month().hour().minute())) \(h.artist) - \(h.title)", cols - 5))
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

struct StatusBar: View {
    let m: Model
    let cols: Int
    var body: some View {
        let state = m.current.map { "\(m.playing ? "playing" : "stopped") \($0.key)" } ?? "idle"
        let left = " \(state) │ \(m.key.isEmpty ? Copy.noKeyShort : Copy.premium) │ \(m.likes.count) liked"
        let legend = cols >= left.count + Copy.legend.count + 10 ? Copy.legend : ""
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
