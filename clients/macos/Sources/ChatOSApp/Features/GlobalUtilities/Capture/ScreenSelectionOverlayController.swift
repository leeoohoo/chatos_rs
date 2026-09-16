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
    let image: CGImage
}

/// Applies the common behavior for windows that must stay above every normal
/// application window during an active screenshot workflow.
///
/// `NSPanel.isFloatingPanel` mutates the panel level to `.floating`, so the
/// screenshot level must deliberately be assigned *after* that property.
@MainActor
func configureScreenshotOverlayPanel(_ panel: NSPanel, levelOffset: Int = 0) {
    panel.isOpaque = false
    panel.backgroundColor = .clear
    panel.isReleasedWhenClosed = false
    panel.collectionBehavior = [
        .canJoinAllSpaces,
        .fullScreenAuxiliary,
        .ignoresCycle,
    ]
    panel.animationBehavior = .none
    panel.isFloatingPanel = true
    panel.hidesOnDeactivate = false
    panel.becomesKeyOnlyIfNeeded = false
    panel.level = NSWindow.Level(
        rawValue: NSWindow.Level.screenSaver.rawValue + levelOffset
    )
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
    private let frozenCaptures: [CGDirectDisplayID: CGImage]

    init(
        isEnglish: Bool,
        frozenCaptures: [CGDirectDisplayID: CGImage]
    ) {
        self.isEnglish = isEnglish
        self.frozenCaptures = frozenCaptures
    }

    func present() {
        guard windows.isEmpty else { return }
        isFinishing = false

        for screen in NSScreen.screens {
            guard let displayID = Self.displayID(for: screen),
                  let frozenImage = frozenCaptures[displayID] else { continue }
            let window = ScreenSelectionWindow(
                contentRect: screen.frame,
                styleMask: [.borderless, .nonactivatingPanel],
                backing: .buffered,
                defer: false,
                screen: screen
            )
            let view = ScreenSelectionOverlayView(
                frozenImage: frozenImage,
                isEnglish: isEnglish
            )
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
            configureScreenshotOverlayPanel(window)
            window.hasShadow = false
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

        NSCursor.crosshair.push()
        guard !windows.isEmpty else {
            cancel()
            return
        }

        // Do not make the selection panel key. Transient windows in the app
        // being captured commonly close as soon as their key window resigns.
        windows.forEach { $0.orderFrontRegardless() }
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
        views.forEach {
            $0.updateSelection(nil, active: $0 === view)
        }
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
            activeView = nil
            view.updateSelection(nil, active: false)
            return
        }

        finishSelection(localRect: localRect, window: window, screen: screen)
    }

    private func finishSelection(
        localRect: NSRect,
        window: NSWindow,
        screen: NSScreen
    ) {
        let globalRect = window.convertToScreen(localRect)
        guard let displayID = Self.displayID(for: screen),
              let frozenImage = frozenCaptures[displayID] else {
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
            displayID: displayID,
            sourceRect: sourceRect,
            outputSize: CGSize(
                width: globalRect.width * scale,
                height: globalRect.height * scale
            )
        )
        guard let image = Self.crop(
            frozenImage,
            to: captureRegion.sourceRect,
            displaySize: screen.frame.size
        ) else {
            cancel()
            return
        }
        let result = ScreenSelection(
            screen: screen,
            globalRect: globalRect,
            captureRegion: captureRegion,
            image: image
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

    static func crop(
        _ image: CGImage,
        to sourceRect: CGRect,
        displaySize: CGSize
    ) -> CGImage? {
        guard displaySize.width > 0, displaySize.height > 0 else { return nil }
        let scaleX = CGFloat(image.width) / displaySize.width
        let scaleY = CGFloat(image.height) / displaySize.height
        let pixelRect = CGRect(
            x: sourceRect.minX * scaleX,
            y: sourceRect.minY * scaleY,
            width: sourceRect.width * scaleX,
            height: sourceRect.height * scaleY
        ).integral.intersection(CGRect(
            x: 0,
            y: 0,
            width: image.width,
            height: image.height
        ))
        guard !pixelRect.isNull, pixelRect.width > 0, pixelRect.height > 0 else {
            return nil
        }
        return image.cropping(to: pixelRect)
    }

    private static func displayID(for screen: NSScreen) -> CGDirectDisplayID? {
        guard let number = screen.deviceDescription[
            NSDeviceDescriptionKey("NSScreenNumber")
        ] as? NSNumber else { return nil }
        return CGDirectDisplayID(number.uint32Value)
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

final class ScreenSelectionWindow: NSPanel {
    var onCancel: (() -> Void)?

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }

    override func cancelOperation(_ sender: Any?) {
        onCancel?()
    }
}
