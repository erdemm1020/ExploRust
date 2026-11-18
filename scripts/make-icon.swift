#!/usr/bin/env swift
// Draws the app icon and writes assets/AppIcon.icns.
// Usage: swift scripts/make-icon.swift   (run again only if the design changes)
import AppKit

func render(size: Int) -> Data {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let s = CGFloat(size)

    // macOS icon grid: the squircle fills 80% of the canvas.
    let tile = NSRect(x: s * 0.1, y: s * 0.1, width: s * 0.8, height: s * 0.8)
    let squircle = NSBezierPath(roundedRect: tile, xRadius: s * 0.18, yRadius: s * 0.18)

    let shadow = NSShadow()
    shadow.shadowColor = NSColor.black.withAlphaComponent(0.25)
    shadow.shadowBlurRadius = s * 0.02
    shadow.shadowOffset = NSSize(width: 0, height: -s * 0.012)
    NSGraphicsContext.saveGraphicsState()
    shadow.set()
    NSColor(calibratedRed: 0.04, green: 0.41, blue: 0.94, alpha: 1).setFill()
    squircle.fill()
    NSGraphicsContext.restoreGraphicsState()

    NSGradient(
        starting: NSColor(calibratedRed: 0.36, green: 0.72, blue: 1.0, alpha: 1),
        ending: NSColor(calibratedRed: 0.04, green: 0.41, blue: 0.94, alpha: 1)
    )!.draw(in: squircle, angle: -90)

    // The folder, drawn on the same 24-unit grid as assets/icons/folder.svg.
    let g = tile.width * 0.62 / 24
    let ox = tile.midX - 12 * g
    let oy = tile.midY - 12 * g
    func r(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat, _ radius: CGFloat) -> NSBezierPath {
        // y grows downward on the grid, upward in AppKit.
        NSBezierPath(
            roundedRect: NSRect(x: ox + x * g, y: oy + (24 - y - h) * g, width: w * g, height: h * g),
            xRadius: radius * g, yRadius: radius * g)
    }
    let folderShadow = NSShadow()
    folderShadow.shadowColor = NSColor.black.withAlphaComponent(0.2)
    folderShadow.shadowBlurRadius = s * 0.015
    folderShadow.shadowOffset = NSSize(width: 0, height: -s * 0.008)
    NSGraphicsContext.saveGraphicsState()
    folderShadow.set()
    NSColor.white.withAlphaComponent(0.6).setFill()
    // One shape (tab + body), so the translucent parts do not stack up.
    let back = r(3, 4, 9.5, 6, 2)
    back.append(r(3, 6.5, 18, 13.5, 2.5))
    back.fill()
    NSColor.white.setFill()
    r(3, 9, 18, 11, 2.5).fill()
    NSGraphicsContext.restoreGraphicsState()

    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

let iconset = "build/AppIcon.iconset"
try? FileManager.default.removeItem(atPath: iconset)
try! FileManager.default.createDirectory(atPath: iconset, withIntermediateDirectories: true)
for base in [16, 32, 128, 256, 512] {
    try! render(size: base).write(to: URL(fileURLWithPath: "\(iconset)/icon_\(base)x\(base).png"))
    try! render(size: base * 2).write(to: URL(fileURLWithPath: "\(iconset)/icon_\(base)x\(base)@2x.png"))
}
let p = Process()
p.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
p.arguments = ["-c", "icns", iconset, "-o", "assets/AppIcon.icns"]
try! p.run()
p.waitUntilExit()
try? FileManager.default.removeItem(atPath: iconset)
print("wrote assets/AppIcon.icns")
