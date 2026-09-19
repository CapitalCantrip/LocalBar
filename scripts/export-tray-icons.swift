#!/usr/bin/env swift
// Renders four SF Symbol variants to PNG files used as Tauri tray icons.
// Run from the repo root: swift scripts/export-tray-icons.swift
import AppKit
import Foundation

let symbols: [(symbol: String, output: String)] = [
    ("brain.head.profile",                       "tray-idle"),
    ("brain.head.profile.fill",                  "tray-running"),
    ("exclamationmark.triangle.fill",            "tray-error"),
    ("arrow.trianglehead.2.clockwise.rotate.90", "tray-transitioning"),
]

let pointSize: CGFloat = 14
let canvasPoints: CGFloat = 18

let symConfig = NSImage.SymbolConfiguration(pointSize: pointSize, weight: .regular, scale: .medium)

for (symbolName, baseName) in symbols {
    guard let raw = NSImage(systemSymbolName: symbolName, accessibilityDescription: nil),
          let sym = raw.withSymbolConfiguration(symConfig) else {
        fputs("❌ symbol not found: \(symbolName)\n", stderr)
        continue
    }

    let canvas = NSImage(size: NSSize(width: canvasPoints, height: canvasPoints))
    canvas.lockFocusFlipped(false)
    NSColor.clear.setFill()
    NSRect(origin: .zero, size: canvas.size).fill()
    let symSize = sym.size
    let origin = NSPoint(
        x: (canvasPoints - symSize.width) / 2,
        y: (canvasPoints - symSize.height) / 2
    )
    NSColor.black.set()
    sym.draw(at: origin, from: .zero, operation: .sourceOver, fraction: 1.0)
    canvas.unlockFocus()

    guard let tiff = canvas.tiffRepresentation,
          let bitmap = NSBitmapImageRep(data: tiff),
          let png = bitmap.representation(using: .png, properties: [:]) else {
        fputs("❌ PNG conversion failed: \(symbolName)\n", stderr)
        continue
    }

    let path = "localbar-tauri/icons/\(baseName).png"
    do {
        try png.write(to: URL(fileURLWithPath: path))
        print("✓ \(path) (\(png.count) bytes)")
    } catch {
        fputs("❌ write failed \(path): \(error)\n", stderr)
    }
}
