@preconcurrency import AppKit
@preconcurrency import CoreGraphics
import Foundation

@MainActor
final class VirtualCursorOverlay: @unchecked Sendable {
    private final class OverlayWindow: NSWindow {
        override var canBecomeKey: Bool { false }
        override var canBecomeMain: Bool { false }
    }

    private final class CursorView: NSView {
        var cursor: CGPoint?
        var trail: [CGPoint] = []

        override var isOpaque: Bool { false }

        override func draw(_ dirtyRect: NSRect) {
            super.draw(dirtyRect)
            guard let context = NSGraphicsContext.current?.cgContext else { return }
            Self.drawTrail(trail, in: context)
            if let cursor {
                Self.drawPointer(at: cursor, in: context)
            }
        }

        private static func drawTrail(
            _ points: [CGPoint],
            in context: CGContext
        ) {
            CursorArtwork.drawTrail(points, in: context)
        }

        private static func drawPointer(
            at point: CGPoint,
            in context: CGContext
        ) {
            CursorArtwork.drawPointer(at: point, size: 27, in: context)
        }
    }

    private struct Surface {
        let window: NSWindow
        let view: CursorView
    }

    private var surfaces: [UInt32: Surface] = [:]

    init() {
        _ = NSApplication.shared
    }

    func show(
        cursor: PointDTO,
        trail: [PointDTO],
        displays: [DisplayDTO]
    ) {
        synchronizeSurfaces(displays: displays)
        for display in displays {
            guard let surface = surfaces[display.id] else { continue }
            surface.view.cursor = localPoint(cursor, display: display)
            surface.view.trail = trail.compactMap {
                localPoint($0, display: display)
            }
            surface.view.needsDisplay = true
            surface.view.displayIfNeeded()
            surface.window.orderFrontRegardless()
        }
    }

    func hide() {
        for surface in surfaces.values {
            surface.window.orderOut(nil)
        }
    }

    private func synchronizeSurfaces(displays: [DisplayDTO]) {
        let validIDs = Set(displays.map(\.id))
        for id in surfaces.keys where !validIDs.contains(id) {
            surfaces[id]?.window.close()
            surfaces.removeValue(forKey: id)
        }

        for display in displays where surfaces[display.id] == nil {
            guard let screen = NSScreen.screens.first(where: {
                ($0.deviceDescription[
                    NSDeviceDescriptionKey("NSScreenNumber")
                ] as? NSNumber)?.uint32Value == display.id
            }) else { continue }

            let window = OverlayWindow(
                contentRect: screen.frame,
                styleMask: [.borderless],
                backing: .buffered,
                defer: false,
                screen: screen
            )
            let view = CursorView(frame: NSRect(origin: .zero, size: screen.frame.size))
            view.wantsLayer = true
            view.layer?.backgroundColor = NSColor.clear.cgColor
            window.contentView = view
            window.backgroundColor = .clear
            window.isOpaque = false
            window.hasShadow = false
            window.ignoresMouseEvents = true
            window.acceptsMouseMovedEvents = false
            window.level = .screenSaver
            window.collectionBehavior = [
                .canJoinAllSpaces,
                .fullScreenAuxiliary,
                .stationary,
                .ignoresCycle
            ]
            window.sharingType = .none
            surfaces[display.id] = Surface(window: window, view: view)
        }
    }

    private func localPoint(
        _ point: PointDTO,
        display: DisplayDTO
    ) -> CGPoint? {
        guard ComputerController.contains(display.frame, point) else { return nil }
        return CGPoint(
            x: point.x - display.frame.x,
            y: display.frame.height - (point.y - display.frame.y)
        )
    }
}
