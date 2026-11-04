import SwiftUI
import UniformTypeIdentifiers

enum Phosphor {
    static let amber = Color(red: 1.00, green: 0.66, blue: 0.12)
    static let bright = Color(red: 1.00, green: 0.82, blue: 0.35)
    static let dim = Color(red: 0.60, green: 0.38, blue: 0.08)
    static let bg = Color(red: 0.07, green: 0.045, blue: 0.03)
    static let font = Font.custom("Px437 IBM VGA 9x16", fixedSize: 16)
}

/// Fit a string to exactly n cells.
func pad(_ s: String, _ n: Int) -> String {
    let t = String(s.prefix(max(n, 0)))
    return t + String(repeating: " ", count: max(n - t.count, 0))
}

func hm(_ s: Int) -> String { s < 60 ? "" : s < 3600 ? "\(s / 60)m" : "\(s / 3600)h\(s % 3600 / 60)m" }

extension View {
    /// One text row on the 16pt grid.
    func row() -> some View { frame(maxWidth: .infinity, alignment: .leading).frame(height: 16) }

    func crt() -> some View {
        visualEffect { content, proxy in
            content
                .layerEffect(ShaderLibrary.bundle(.module).phosphor(.float2(proxy.size)), maxSampleOffset: CGSize(width: 4, height: 4))
                .distortionEffect(ShaderLibrary.bundle(.module).curve(.float2(proxy.size), .float(0.045)), maxSampleOffset: CGSize(width: 80, height: 60))
        }
    }
}

struct ContentView: View {
    let m: Model
    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            Screen(m: m)
                .clipShape(RoundedRectangle(cornerRadius: 14))
                .padding(.top, 30)
                .padding([.horizontal, .bottom], 18)
                .crt()
        }
        .preferredColorScheme(.dark)
        .tint(Phosphor.amber)
    }
}

struct Screen: View {
    let m: Model
    var body: some View {
        GeometryReader { g in
            let cols = Int(g.size.width / 9)
            VStack(spacing: 0) {
                HStack(spacing: 0) {
                    Box(title: Copy.app) { c, _ in ChannelPane(m: m, cols: c) }.frame(width: 36 * 9)
                    Box(title: Copy.now) { c, _ in NowPane(m: m, cols: c) }
                }
                StatusBar(m: m, cols: cols)
            }
        }
        .font(Phosphor.font)
        .foregroundStyle(Phosphor.amber)
        .background(Phosphor.bg)
    }
}

/// Double-line DOS box drawn with real box characters; content gets its inner size in cells.
struct Box<Content: View>: View {
    let title: String
    @ViewBuilder let content: (Int, Int) -> Content
    var body: some View {
        GeometryReader { g in
            let cols = Int(g.size.width / 9)
            let rows = max(Int(g.size.height / 16) - 2, 0)
            let inner = max(cols - 2, 0)
            VStack(alignment: .leading, spacing: 0) {
                Text("╔═[ \(title) ]" + String(repeating: "═", count: max(inner - title.count - 5, 0)) + "╗").row()
                HStack(spacing: 0) {
                    side(rows)
                    content(inner, rows)
                        .frame(width: CGFloat(inner) * 9, height: CGFloat(rows) * 16, alignment: .topLeading)
                        .clipped()
                    side(rows)
                }
                Text("╚" + String(repeating: "═", count: inner) + "╝").row()
            }
        }
    }
    private func side(_ rows: Int) -> some View {
        VStack(spacing: 0) { ForEach(0..<rows, id: \.self) { _ in Text("║").frame(height: 16) } }
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
        }.buttonStyle(.plain)
    }
}

struct ChannelPane: View {
    @Bindable var m: Model
    let cols: Int
    @AppStorage("key") private var key = ""
    var body: some View {
        VStack(spacing: 0) {
            GridInput(prompt: Copy.find, text: $m.query, placeholder: Copy.search, focusTrigger: m.focusSearch).row()
            HStack(spacing: 0) {
                Text(" ")
                Btn(label: "[ \(Copy.techno) ]", on: !m.showAll) { m.showAll = false }
                Text(" ")
                Btn(label: "[ \(Copy.all) ]", on: m.showAll) { m.showAll = true }
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
            GridInput(prompt: Copy.keyPrompt, text: $key, secret: true, placeholder: Copy.keyPlaceholder).row()
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
    @State private var tab = 0
    @State private var heardQuery = ""
    var body: some View {
        let artCols = 21
        let w = cols - artCols - 1
        VStack(alignment: .leading, spacing: 0) {
            if let c = m.current {
                HStack(alignment: .top, spacing: 0) {
                    Art(url: img(m.nowArtPath, 96) ?? img(c.asset_url, 96), w: CGFloat(artCols) * 9, h: 12 * 16)
                    Text(" ")
                    VStack(alignment: .leading, spacing: 0) {
                        Text(pad(" DI.FM " + c.name.uppercased(), w)).foregroundStyle(Phosphor.bright).row()
                        Text("").row()
                        Text(" " + (m.nowTrack?.title ?? Copy.nothing)).foregroundStyle(Phosphor.bright).lineLimit(3)
                            .frame(width: CGFloat(w) * 9, height: 48, alignment: .topLeading)
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
                }.frame(height: 12 * 16)
            } else {
                Text(pad(" " + Copy.idle, cols)).foregroundStyle(Phosphor.bright).row()
                    .frame(height: 12 * 16, alignment: .top)
            }
            Text("").row()
            HStack(spacing: 0) {
                Text(" ")
                Btn(label: "[ \(Copy.lastOn) ]", on: tab == 0) { tab = 0 }
                Text(" ")
                Btn(label: "[ \(Copy.likes) ]", on: tab == 1) { tab = 1 }
                Text(" ")
                Btn(label: "[ \(Copy.heard) ]", on: tab == 2) { tab = 2 }
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

/// Text input on the character grid with a blinking block cursor. Typing, backspace, escape clears, paste works.
struct GridInput: View {
    let prompt: String
    @Binding var text: String
    var secret = false
    var placeholder = ""
    var focusTrigger = 0
    @FocusState private var focused: Bool
    var body: some View {
        let shown = secret ? String(repeating: "*", count: text.count) : text
        HStack(spacing: 0) {
            Text(prompt)
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
        }
        .contentShape(Rectangle())
        .focusable(true, interactions: .edit).focusEffectDisabled()
        .focused($focused)
        .onTapGesture { focused = true }
        .onChange(of: focusTrigger) { focused = true }
        .onKeyPress(phases: .down) { p in
            if p.modifiers.contains(.command) || p.modifiers.contains(.control) { return .ignored }
            switch p.key {
            case .delete: if !text.isEmpty { text.removeLast() }; return .handled
            case .escape: text = ""; return .handled
            default:
                guard p.characters.count == 1, let c = p.characters.first, !c.isNewline, c != "\t",
                      c.unicodeScalars.first.map({ $0.value >= 32 }) == true else { return .ignored }
                text.append(c)
                return .handled
            }
        }
        .onPasteCommand(of: [.text]) { providers in
            providers.first?.loadObject(ofClass: NSString.self) { s, _ in
                guard let s = s as? String else { return }
                DispatchQueue.main.async { text += s.trimmingCharacters(in: .whitespacesAndNewlines) }
            }
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
                    .row().contentShape(Rectangle())
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
                        .row().contentShape(Rectangle())
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
        let left = " \(Copy.app) │ \(state) │ \(m.key.isEmpty ? Copy.free : Copy.premium) │ \(m.likes.count) liked"
        Text(pad(left, cols - Copy.legend.count) + Copy.legend)
            .foregroundStyle(Phosphor.bg).background(Phosphor.amber).row()
    }
}
