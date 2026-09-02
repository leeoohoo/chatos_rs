import AppKit
@preconcurrency import Carbon
import ChatOSConnector
import CoreGraphics

private extension Notification.Name {
    static let chatOSScreenSelectionEscapePressed = Notification.Name(
        "com.chatos.screen-selection-escape-pressed"
    )
}

private let screenshotEscapeSignature: OSType = 0x4345_5343
private let screenshotEscapeIdentifier: UInt32 = 1

private let screenSelectionEscapeHandler: EventHandlerUPP = { _, eventRef, _ in
    guard let eventRef else { return OSStatus(eventNotHandledErr) }
    var hotKeyID = EventHotKeyID()
    let status = GetEventParameter(
        eventRef,
        EventParamName(kEventParamDirectObject),
        EventParamType(typeEventHotKeyID),
        nil,
        MemoryLayout<EventHotKeyID>.size,
        nil,
        &hotKeyID
    )
    guard status == noErr,
          hotKeyID.signature == screenshotEscapeSignature,
          hotKeyID.id == screenshotEscapeIdentifier else {
        return OSStatus(eventNotHandledErr)
    }
    DispatchQueue.main.async {
        NotificationCenter.default.post(
            name: .chatOSScreenSelectionEscapePressed,
            object: nil
        )
    }
    return noErr
}

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
    private var escapeHotKeyMonitor: ScreenshotEscapeHotKeyMonitor?
    private var isFinishing = false
    private let isEnglish: Bool
    private var windowCandidates: [ScreenshotWindowCandidate] = []
    private var initialWindowRect: NSRect?

    init(isEnglish: Bool) {
        self.isEnglish = isEnglish
    }

    func present() {
        guard windows.isEmpty else { return }
        isFinishing = false
        windowCandidates = ScreenshotWindowCandidate.visibleWindows()

        for screen in NSScreen.screens {
            let window = ScreenSelectionWindow(
                contentRect: screen.frame,
                styleMask: [.borderless, .nonactivatingPanel],
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
            view.onPointerMoved = { [weak self] view, point in
                self?.updateHoveredWindow(in: view, at: point)
            }
            view.onPointerExited = { view in
                view.updateHoveredWindow(nil)
            }

            window.contentView = view
            window.isOpaque = false
            window.backgroundColor = .clear
            window.hasShadow = false
            window.level = .screenSaver
            window.collectionBehavior = [
                .canJoinAllSpaces,
                .fullScreenAuxiliary,
                .ignoresCycle,
            ]
            window.isReleasedWhenClosed = false
            window.animationBehavior = .none
            window.isFloatingPanel = true
            window.hidesOnDeactivate = false
            window.becomesKeyOnlyIfNeeded = false
            window.onCancel = { [weak self] in self?.cancel() }
            windows.append(window)
            views.append(view)
        }

        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard event.keyCode == 53 else { return event }
            self?.cancel()
            return nil
        }
        let escapeHotKeyMonitor = ScreenshotEscapeHotKeyMonitor { [weak self] in
            self?.cancel()
        }
        escapeHotKeyMonitor.start()
        self.escapeHotKeyMonitor = escapeHotKeyMonitor

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
        initialWindowRect = view.hoveredWindowRect
        views.forEach {
            $0.updateSelection($0 === view ? initialWindowRect : nil, active: $0 === view)
        }
    }

    private func changeSelection(in view: ScreenSelectionOverlayView, to point: NSPoint) {
        guard view === activeView, let dragOrigin else { return }
        if hypot(point.x - dragOrigin.x, point.y - dragOrigin.y) < 4,
           let initialWindowRect {
            view.updateSelection(initialWindowRect, active: true)
            return
        }
        initialWindowRect = nil
        view.updateSelection(normalizedRect(from: dragOrigin, to: point), active: true)
    }

    private func completeSelection(in view: ScreenSelectionOverlayView, at point: NSPoint) {
        guard !isFinishing,
              view === activeView,
              let dragOrigin,
              let window = view.window,
              let screen = window.screen else { return }

        let draggedRect = normalizedRect(from: dragOrigin, to: point)
        let localRect = (
            draggedRect.width < 4 && draggedRect.height < 4
                ? initialWindowRect ?? draggedRect
                : draggedRect
        ).integral
        guard localRect.width >= 4, localRect.height >= 4 else {
            self.dragOrigin = nil
            initialWindowRect = nil
            activeView = nil
            view.updateSelection(nil, active: false)
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
        let selectionOverlayWindowIDs = windows.compactMap { window in
            window.windowNumber > 0 ? CGWindowID(window.windowNumber) : nil
        }
        let captureRegion = NativeScreenCaptureRegion(
            displayID: CGDirectDisplayID(displayID.uint32Value),
            sourceRect: sourceRect,
            outputSize: CGSize(
                width: globalRect.width * scale,
                height: globalRect.height * scale
            ),
            excludedWindowIDs: selectionOverlayWindowIDs
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
        escapeHotKeyMonitor?.stop()
        escapeHotKeyMonitor = nil
        windows.forEach { $0.orderOut(nil) }
        windows.removeAll()
        views.removeAll()
        activeView = nil
        dragOrigin = nil
        initialWindowRect = nil
        windowCandidates.removeAll()
        NSCursor.pop()
    }

    private func updateHoveredWindow(in view: ScreenSelectionOverlayView, at point: NSPoint) {
        guard !isFinishing, activeView == nil, let window = view.window else { return }
        let globalPoint = window.convertToScreen(NSRect(origin: point, size: .zero)).origin
        let candidate = windowCandidates.first { $0.rect.contains(globalPoint) }
        guard let candidate else {
            view.updateHoveredWindow(nil)
            return
        }
        let clipped = candidate.rect.intersection(window.frame)
        guard !clipped.isNull, !clipped.isEmpty else {
            view.updateHoveredWindow(nil)
            return
        }
        view.updateHoveredWindow(window.convertFromScreen(clipped))
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

@MainActor
final class ScreenshotEscapeHotKeyMonitor {
    private let onEscape: () -> Void
    private var handlerRef: EventHandlerRef?
    private var hotKeyRef: EventHotKeyRef?
    private var observer: NSObjectProtocol?

    init(onEscape: @escaping () -> Void) {
        self.onEscape = onEscape
    }

    func start() {
        guard handlerRef == nil, hotKeyRef == nil else { return }
        observer = NotificationCenter.default.addObserver(
            forName: .chatOSScreenSelectionEscapePressed,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.onEscape()
            }
        }

        var eventType = EventTypeSpec(
            eventClass: OSType(kEventClassKeyboard),
            eventKind: UInt32(kEventHotKeyPressed)
        )
        let installStatus = InstallEventHandler(
            GetApplicationEventTarget(),
            screenSelectionEscapeHandler,
            1,
            &eventType,
            nil,
            &handlerRef
        )
        guard installStatus == noErr else {
            stop()
            return
        }

        var registration: EventHotKeyRef?
        let identifier = EventHotKeyID(
            signature: screenshotEscapeSignature,
            id: screenshotEscapeIdentifier
        )
        let registerStatus = RegisterEventHotKey(
            UInt32(kVK_Escape),
            0,
            identifier,
            GetApplicationEventTarget(),
            0,
            &registration
        )
        guard registerStatus == noErr, let registration else {
            stop()
            return
        }
        hotKeyRef = registration
    }

    func stop() {
        if let hotKeyRef {
            UnregisterEventHotKey(hotKeyRef)
            self.hotKeyRef = nil
        }
        if let handlerRef {
            RemoveEventHandler(handlerRef)
            self.handlerRef = nil
        }
        if let observer {
            NotificationCenter.default.removeObserver(observer)
            self.observer = nil
        }
    }
}

private struct ScreenshotWindowCandidate {
    let rect: CGRect

    static func visibleWindows() -> [ScreenshotWindowCandidate] {
        guard let windowInfo = CGWindowListCopyWindowInfo(
            [.optionOnScreenOnly, .excludeDesktopElements],
            kCGNullWindowID
        ) as? [[CFString: Any]],
        let mainScreen = NSScreen.screens.first else {
            return []
        }

        let currentPID = getpid()
        return windowInfo.compactMap { info in
            guard (info[kCGWindowOwnerPID] as? NSNumber)?.int32Value != currentPID,
                  (info[kCGWindowLayer] as? NSNumber)?.intValue == 0,
                  ((info[kCGWindowAlpha] as? NSNumber)?.doubleValue ?? 1) > 0.01,
                  let bounds = info[kCGWindowBounds] as? [String: Any],
                  let quartzRect = CGRect(dictionaryRepresentation: bounds as CFDictionary),
                  quartzRect.width >= 40,
                  quartzRect.height >= 30 else {
                return nil
            }

            let appKitRect = CGRect(
                x: quartzRect.minX,
                y: mainScreen.frame.maxY - quartzRect.maxY,
                width: quartzRect.width,
                height: quartzRect.height
            )
            return ScreenshotWindowCandidate(rect: appKitRect)
        }
    }
}

private final class ScreenSelectionWindow: NSPanel {
    var onCancel: (() -> Void)?

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func cancelOperation(_ sender: Any?) {
        onCancel?()
    }
}
