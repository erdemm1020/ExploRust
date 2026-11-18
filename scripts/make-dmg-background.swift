#!/usr/bin/env swift
// Draws the picture behind the icons in the DMG window and writes
// assets/dmg-background.tiff (1x + 2x in one file, so it is sharp on Retina).
// Usage: swift scripts/make-dmg-background.swift   (only if the design changes)
import AppKit

let width: CGFloat = 660
let height: CGFloat = 400

func render(scale: Int) -> Data {
    let px = Int(width) * scale
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: Int(height) * scale, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0)!
    // Draw in points: the bitmap is 1x or 2x, the picture stays the same size.
    rep.size = NSSize(width: width, height: height)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let full = NSRect(x: 0, y: 0, width: width, height: height)

    // Mid-tone blue: both black and white Finder labels stay readable on it,
    // in light and in dark mode.
    NSGradient(
        starting: NSColor(calibratedRed: 0.42, green: 0.67, blue: 0.97, alpha: 1),
        ending: NSColor(calibratedRed: 0.23, green: 0.51, blue: 0.90, alpha: 1)
    )!.draw(in: full, angle: -90)

    // Soft light from the top left.
    NSGradient(
        starting: NSColor.white.withAlphaComponent(0.28),
        ending: NSColor.white.withAlphaComponent(0)
    )!.draw(fromCenter: NSPoint(x: 90, y: height - 40), radius: 0,
            toCenter: NSPoint(x: 90, y: height - 40), radius: 380, options: [])

    // A few faint folder-tab shapes, echoing the icon.
    NSColor.white.withAlphaComponent(0.07).setFill()
    NSBezierPath(roundedRect: NSRect(x: 470, y: 250, width: 250, height: 170), xRadius: 26, yRadius: 26).fill()
    NSBezierPath(roundedRect: NSRect(x: -60, y: -40, width: 230, height: 150), xRadius: 26, yRadius: 26).fill()

    func text(_ s: String, size: CGFloat, weight: NSFont.Weight, alpha: CGFloat, y: CGFloat) {
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(0.22)
        shadow.shadowBlurRadius = 6
        shadow.shadowOffset = NSSize(width: 0, height: -1)
        let attrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: size, weight: weight),
            .foregroundColor: NSColor.white.withAlphaComponent(alpha),
            .shadow: shadow,
        ]
        let str = NSAttributedString(string: s, attributes: attrs)
        let w = str.size().width
        str.draw(at: NSPoint(x: (width - w) / 2, y: y))
    }
    text("Install ExploRust", size: 28, weight: .semibold, alpha: 1, y: height - 70)
    text("Drag the app onto the Applications folder", size: 13.5, weight: .regular, alpha: 0.9, y: height - 98)

    // The arrow between the two icons (icons sit at x 165 and x 495, y 195 from the top).
    let cy = height - 195
    let arrow = NSBezierPath()
    arrow.lineWidth = 7
    arrow.lineCapStyle = .round
    arrow.lineJoinStyle = .round
    arrow.move(to: NSPoint(x: 262, y: cy))
    arrow.line(to: NSPoint(x: 395, y: cy))
    let head = NSBezierPath()
    head.lineWidth = 7
    head.lineCapStyle = .round
    head.lineJoinStyle = .round
    head.move(to: NSPoint(x: 372, y: cy + 24))
    head.line(to: NSPoint(x: 398, y: cy))
    head.line(to: NSPoint(x: 372, y: cy - 24))
    let glow = NSShadow()
    glow.shadowColor = NSColor.black.withAlphaComponent(0.25)
    glow.shadowBlurRadius = 8
    glow.shadowOffset = NSSize(width: 0, height: -2)
    NSGraphicsContext.saveGraphicsState()
    glow.set()
    NSColor.white.withAlphaComponent(0.95).setStroke()
    arrow.stroke()
    head.stroke()
    NSGraphicsContext.restoreGraphicsState()

    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

let fm = FileManager.default
try? fm.createDirectory(atPath: "build", withIntermediateDirectories: true)
try! render(scale: 1).write(to: URL(fileURLWithPath: "build/dmg-bg.png"))
try! render(scale: 2).write(to: URL(fileURLWithPath: "build/dmg-bg@2x.png"))
let p = Process()
p.executableURL = URL(fileURLWithPath: "/usr/bin/tiffutil")
p.arguments = ["-cathidpicheck", "build/dmg-bg.png", "build/dmg-bg@2x.png", "-out", "assets/dmg-background.tiff"]
try! p.run()
p.waitUntilExit()
print("wrote assets/dmg-background.tiff")
