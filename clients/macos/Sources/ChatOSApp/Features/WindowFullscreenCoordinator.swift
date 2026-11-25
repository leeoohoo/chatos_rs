import AppKit
import SwiftUI

@MainActor
final class WindowFullscreenCoordinator: ObservableObject {
    private struct RestoreState {
        let frame: NSRect
        let contentMinSize: NSSize
        let contentMaxSize: NSSize
    }

    @Published private(set) var isFullScreen = false

    private weak var window: NSWindow?
    private var restoreState: RestoreState?

    func attach(_ window: NSWindow?) {
        guard self.window !== window else { return }
        if isFullScreen {
            exitFullScreen()
        }
        self.window = window
    }

    func toggle() {
        if isFullScreen {
            exitFullScreen()
        } else {
            enterFullScreen()
        }
    }

    func enterFullScreen(in targetFrame: NSRect? = nil) {
        guard !isFullScreen,
              let window,
              let frame = targetFrame ?? window.screen?.visibleFrame ?? NSScreen.main?.visibleFrame
        else { return }
        restoreState = RestoreState(
            frame: window.frame,
            contentMinSize: window.contentMinSize,
            contentMaxSize: window.contentMaxSize
        )
        window.contentMaxSize = frame.size
        window.setFrame(frame, display: true, animate: true)
        isFullScreen = true
    }

    func exitFullScreen() {
        guard isFullScreen, let restoreState else { return }
        window?.contentMinSize = restoreState.contentMinSize
        window?.contentMaxSize = restoreState.contentMaxSize
        window?.setFrame(restoreState.frame, display: true, animate: true)
        self.restoreState = nil
        isFullScreen = false
    }
}

struct WindowFullscreenReader: NSViewRepresentable {
    let coordinator: WindowFullscreenCoordinator

    func makeNSView(context: Context) -> WindowReaderView {
        WindowReaderView { [weak coordinator] window in
            coordinator?.attach(window)
        }
    }

    func updateNSView(_ nsView: WindowReaderView, context: Context) {
        nsView.onWindowChange = { [weak coordinator] window in
            coordinator?.attach(window)
        }
        nsView.reportWindow()
    }
}

final class WindowReaderView: NSView {
    var onWindowChange: (NSWindow?) -> Void

    init(onWindowChange: @escaping (NSWindow?) -> Void) {
        self.onWindowChange = onWindowChange
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        reportWindow()
    }

    func reportWindow() {
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            onWindowChange(window)
        }
    }
}
