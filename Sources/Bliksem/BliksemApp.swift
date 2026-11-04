import CoreText
import SwiftUI

@main
struct BliksemApp: App {
    @State private var m = Model()

    init() {
        if let url = Bundle.module.url(forResource: "Px437_IBM_VGA_9x16", withExtension: "ttf") {
            CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
        }
        // Dev hook: BLIKSEM_SNAP=/path.png renders the screen (minus scroll views and the CRT shader) after 5s,
        // checks the shaders compile, and quits.
        if let path = ProcessInfo.processInfo.environment["BLIKSEM_SNAP"] {
            let model = m
            DispatchQueue.main.asyncAfter(deadline: .now() + 5) {
                let r = ImageRenderer(content: Screen(m: model).frame(width: 1008, height: 640))
                r.scale = 2
                if let cg = r.cgImage {
                    try? NSBitmapImageRep(cgImage: cg).representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
                }
                Task {
                    if #available(macOS 15, *) {
                        let lib = ShaderLibrary.bundle(.module)
                        do {
                            try await lib.phosphor(.float2(CGSize(width: 1, height: 1))).compile(as: .layerEffect)
                            try await lib.curve(.float2(CGSize(width: 1, height: 1)), .float(0.045)).compile(as: .distortionEffect)
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
        .defaultSize(width: 1044, height: 720)
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
