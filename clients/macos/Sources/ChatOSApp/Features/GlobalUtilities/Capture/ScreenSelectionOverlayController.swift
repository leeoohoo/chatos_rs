import AppKit
import ChatOSConnector
import CoreGraphics

struct ScreenSelection {
    let screen: NSScreen
    let globalRect: CGRect
    let captureRegion: NativeScreenCaptureRegion
}

@MainActor
final class ScreenSelectionOverlayController {
    var onComplete: ((ScreenSelection) -> Void)?
    var onCancel: (() -> Void)?

    private var windows: [NSWindow] = []
    private var views: [ScreenSelectionOverlayView] = []
    private weak var activeView: ScreenSelectionOverlayView?
    private var dragOrigin: NSPoint?
    private var keyMonitor: Any?
    private var isFinishing = false
    private let isEnglish: Bool

    init(isEnglish: Bool) {
        self.isEnglish = isEnglish
    }

    func present() {
        guard windows.isEmpty else { return }
        isFinishing = false

        for screen in NSScreen.screens {
            let window = ScreenSelectionWindow(
                contentRect: screen.frame,
                styleMask: [.borderless],
                backing: .buffered,
                defer: false,
                screen: screen
            )
            let view = ScreenSelectionOverlayView(isEnglish: isEnglish)
            view.frame = NSRect(origin: .zero, size: screen.frame.size)
            view.autoresizingMask = [.width, .height]
            view.onSelectionBegan = { [weak self] view, point in
                self?.beginSelection(in: view, at: point)
            }
            view.onSelectionChanged = { [weak self] view, point in
                self?.changeSelection(in: view, to: point)
            }
            view.onSelectionCompleted = { [weak self] view, point in
                self?.completeSelection(in: view, at: point)
            }

            window.contentView = view
            window.isOpaque = false
            window.backgroundColor = .clear
            window.hasShadow = false
            window.level = .screenSaver
            window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
            window.isReleasedWhenClosed = false
            window.animationBehavior = .none
            window.onCancel = { [weak self] in self?.cancel() }
            windows.append(window)
            views.append(view)
        }

        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard event.keyCode == 53 else { return event }
            self?.cancel()
            return nil
        }

        NSApp.activate(ignoringOtherApps: true)
        windows.forEach { $0.orderFrontRegardless() }
        let mouse = NSEvent.mouseLocation
        let keyWindow = windows.first { $0.frame.contains(mouse) } ?? windows.first
        keyWindow?.makeKeyAndOrderFront(nil)
        NSCursor.crosshair.push()
    }

    func cancel() {
        guard !isFinishing else { return }
        isFinishing = true
        dismissWindows()
        onCancel?()
    }

    private func beginSelection(in view: ScreenSelectionOverlayView, at point: NSPoint) {
        guard !isFinishing else { return }
        activeView = view
        dragOrigin = point
        views.forEach { $0.updateSelection(nil, active: $0 === view) }
    }

    private func changeSelection(in view: ScreenSelectionOverlayView, to point: NSPoint) {
        guard view === activeView, let dragOrigin else { return }
        view.updateSelection(normalizedRect(from: dragOrigin, to: point), active: true)
    }

    private func completeSelection(in view: ScreenSelectionOverlayView, at point: NSPoint) {
        guard !isFinishing,
              view === activeView,
              let dragOrigin,
              let window = view.window,
              let screen = window.screen else { return }

        let localRect = normalizedRect(from: dragOrigin, to: point).integral
        guard localRect.width >= 4, localRect.height >= 4 else {
            self.dragOrigin = nil
            view.updateSelection(nil, active: true)
            return
        }

        let globalRect = window.convertToScreen(localRect)
        guard let displayID = screen.deviceDescription[
            NSDeviceDescriptionKey("NSScreenNumber")
        ] as? NSNumber else {
            cancel()
            return
        }

        let sourceRect = CGRect(
            x: globalRect.minX - screen.frame.minX,
            y: screen.frame.maxY - globalRect.maxY,
            width: globalRect.width,
            height: globalRect.height
        )
        let scale = screen.backingScaleFactor
        let captureRegion = NativeScreenCaptureRegion(
            displayID: CGDirectDisplayID(displayID.uint32Value),
            sourceRect: sourceRect,
            outputSize: CGSize(
                width: globalRect.width * scale,
                height: globalRect.height * scale
            )
        )
        let result = ScreenSelection(
            screen: screen,
            globalRect: globalRect,
            captureRegion: captureRegion
        )

        isFinishing = true
        dismissWindows()
        DispatchQueue.main.async { [weak self] in
            self?.onComplete?(result)
        }
    }

    private func dismissWindows() {
        if let keyMonitor {
            NSEvent.removeMonitor(keyMonitor)
            self.keyMonitor = nil
        }
        windows.forEach { $0.orderOut(nil) }
        windows.removeAll()
        views.removeAll()
        activeView = nil
        dragOrigin = nil
        NSCursor.pop()
    }

    private func normalizedRect(from start: NSPoint, to end: NSPoint) -> NSRect {
        NSRect(
            x: min(start.x, end.x),
            y: min(start.y, end.y),
            width: abs(end.x - start.x),
            height: abs(end.y - start.y)
        )
    }
}

private final class ScreenSelectionWindow: NSWindow {
    var onCancel: (() -> Void)?

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func cancelOperation(_ sender: Any?) {
        onCancel?()
    }
}
