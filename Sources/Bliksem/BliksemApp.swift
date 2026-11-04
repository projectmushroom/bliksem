import CoreText
import SwiftUI

@main
struct BliksemApp: App {
    // Snapshots and the demo must never connect: DI gives one stream per key and any new connection kicks the old one.
    @State private var m = Model(autoplay: ProcessInfo.processInfo.environment["BLIKSEM_SNAP"] == nil)

    init() {
        if let url = Phosphor.resources.url(forResource: "Px437_IBM_VGA_9x16", withExtension: "ttf") {
            CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
        }
        // Dev hook: BLIKSEM_SNAP=/path.png drives the meters with fake audio, renders the screen without the CRT
        // shader, checks the shaders compile and quits after BLIKSEM_HOLD seconds. It never touches the network stream.
        if let path = ProcessInfo.processInfo.environment["BLIKSEM_SNAP"] {
            let model = m
            let hold = ProcessInfo.processInfo.environment["BLIKSEM_HOLD"].flatMap(Double.init) ?? 5
            model.playing = true
            Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { _ in model.radio.meter.feedDemo() }
            DispatchQueue.main.asyncAfter(deadline: .now() + hold) {
                Task { @MainActor in
                    let id = UserDefaults.standard.object(forKey: "lastChannel") as? Int
                    let channel = model.channels.first { $0.id == id } ?? model.channels.first
                    model.current = channel
                    model.playing = true
                    if let ch = channel { model.history = (try? await DI.history(ch.id)) ?? [] }
                    var art: NSImage?
                    if let u = img(model.nowArtPath ?? channel?.asset_url, 96), let (d, _) = try? await URLSession.shared.data(from: u) { art = NSImage(data: d) }
                    let z = model.zoom
                    let r = ImageRenderer(content: Screen(m: model).frame(width: 1188, height: 752).environment(\.grid, Grid(z: z)).environment(\.snapshot, true).environment(\.snapArt, art))
                    r.scale = 2
                    if let cg = r.cgImage {
                        try? NSBitmapImageRep(cgImage: cg).representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
                    }
                    // The live window too: it goes through the real AsyncImage path, still without the shader.
                    try? await Task.sleep(for: .seconds(2))
                    if let v = NSApp.windows.first(where: { $0.isVisible })?.contentView, let rep = v.bitmapImageRepForCachingDisplay(in: v.bounds) {
                        v.cacheDisplay(in: v.bounds, to: rep)
                        try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path + ".live.png"))
                    }
                    if #available(macOS 15, *) {
                        do {
                            try await Phosphor.shaders.phosphor(.float2(CGSize(width: 1, height: 1)), .float(1)).compile(as: .layerEffect)
                            try await Phosphor.shaders.curve(.float2(CGSize(width: 1, height: 1)), .float(0.045)).compile(as: .distortionEffect)
                            FileHandle.standardError.write(Data("shaders ok\n".utf8))
                        } catch {
                            FileHandle.standardError.write(Data("shader error: \(error)\n".utf8))
                        }
                    }
                    NSApp.terminate(nil)
                }
            }
        }
    }

    var body: some Scene {
        Window(Copy.app, id: "main") {
            ContentView(m: m).frame(minWidth: 960, minHeight: 600)
        }
        .windowStyle(.hiddenTitleBar)
        .defaultSize(width: 1224, height: 800)
        .commands { BliksemCommands(m: m) }

        MenuBarExtra {
            MenuBarMenu(m: m)
        } label: {
            Image(systemName: "bolt.fill")
            Text(String((m.nowTrack.map { "\($0.artist) - \($0.title)" } ?? "").prefix(28)))
        }
    }
}

struct BliksemCommands: Commands {
    let m: Model
    var body: some Commands {
        CommandMenu(Copy.app) {
            Button("Play / Stop") { m.toggle() }.keyboardShortcut("p")
            Button("Like") { m.likeCurrent() }.keyboardShortcut("l")
            Button("Next channel") { m.hop(1) }.keyboardShortcut(.downArrow)
            Button("Previous channel") { m.hop(-1) }.keyboardShortcut(.upArrow)
            Button("Find") { m.focusSearch += 1 }.keyboardShortcut("f")
            Divider()
            Button("Zoom In") { m.zoomIn() }.keyboardShortcut("+")
            Button("Zoom Out") { m.zoomOut() }.keyboardShortcut("-")
            Button("Actual Size") { m.zoomReset() }.keyboardShortcut("0")
            Divider()
            ForEach(Array(m.top.enumerated()), id: \.element.id) { i, c in
                Button(c.name) { m.play(c) }.keyboardShortcut(KeyEquivalent(Character(String(i + 1))))
            }
        }
    }
}

struct MenuBarMenu: View {
    let m: Model
    @Environment(\.openWindow) private var openWindow
    var body: some View {
        Text(m.nowTrack.map { "\($0.artist) - \($0.title)" } ?? Copy.idle)
        Button(m.playing ? "Stop" : "Play") { m.toggle() }
        Button(Copy.like) { m.likeCurrent() }.disabled(m.nowTrack == nil)
        Divider()
        ForEach(m.top) { c in Button(c.name) { m.play(c) } }
        Divider()
        Button(Copy.show) { openWindow(id: "main"); NSApp.activate() }
        Button(Copy.quit) { NSApp.terminate(nil) }
    }
}
