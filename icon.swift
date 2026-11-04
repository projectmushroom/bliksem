// Renders the app icon: a pixel bolt in amber on a warm black tile. Usage: swift icon.swift out.png
import AppKit

let bolt = [
    "......XXXXXX",
    ".....XXXXXX.",
    "....XXXXXX..",
    "...XXXXXX...",
    "..XXXXXXXXXX",
    ".XXXXXXXXXX.",
    "......XXXX..",
    ".....XXXX...",
    "....XXXX....",
    "...XXXX.....",
    "..XXX.......",
    ".XX.........",
    "X...........",
]

let size = 1024
let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8, samplesPerPixel: 4,
                           hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)

NSColor(red: 0.07, green: 0.045, blue: 0.03, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 100, y: 100, width: 824, height: 824), xRadius: 185, yRadius: 185).fill()

let px = 48.0
let w = Double(bolt[0].count) * px, h = Double(bolt.count) * px
let x0 = (Double(size) - w) / 2, y0 = (Double(size) - h) / 2
let amber = NSColor(red: 1.0, green: 0.66, blue: 0.12, alpha: 1)
// glow: a blurred copy under the crisp pixels
for (pass, alpha) in [(14.0, 0.08), (6.0, 0.18), (0.0, 1.0)] {
    amber.withAlphaComponent(alpha).setFill()
    for (r, line) in bolt.enumerated() {
        for (c, ch) in line.enumerated() where ch == "X" {
            NSRect(x: x0 + Double(c) * px - pass, y: y0 + h - Double(r + 1) * px - pass, width: px + 2 * pass, height: px + 2 * pass).fill()
        }
    }
}
// scanlines, like the real thing
NSColor.black.withAlphaComponent(0.25).setFill()
for y in stride(from: 100, to: 924, by: 4) { NSRect(x: 100, y: y, width: 824, height: 2).fill() }

NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
