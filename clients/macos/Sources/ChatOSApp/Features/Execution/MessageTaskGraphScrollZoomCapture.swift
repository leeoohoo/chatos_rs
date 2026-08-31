@preconcurrency import AppKit
import SwiftUI

/// Observes local scroll-wheel events without covering the graph's clickable SwiftUI content.
/// This gives mouse wheels and two-finger trackpad scrolling the same zoom behavior.
struct MessageTaskGraphScrollZoomCapture: NSViewRepresentable {
    let onScroll: (CGFloat, CGPoint) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(onScroll: onScroll)
    }

    func makeNSView(context: Context) -> ScrollCaptureView {
        let view = ScrollCaptureView()
        view.coordinator = context.coordinator
        context.coordinator.installMonitor()
        return view
    }

    func updateNSView(_ nsView: ScrollCaptureView, context: Context) {
        context.coordinator.onScroll = onScroll
        nsView.syncGeometry()
    }

    static func dismantleNSView(_ nsView: ScrollCaptureView, coordinator: Coordinator) {
        coordinator.removeMonitor()
        nsView.coordinator = nil
    }

    final class Coordinator {
        var onScroll: (CGFloat, CGPoint) -> Void
        private var monitor: Any?
        private var windowNumber = -1
        private var frameInWindow = CGRect.zero

        init(onScroll: @escaping (CGFloat, CGPoint) -> Void) {
            self.onScroll = onScroll
        }

        func installMonitor() {
            guard monitor == nil else { return }
            monitor = NSEvent.addLocalMonitorForEvents(matching: .scrollWheel) { [weak self] event in
                guard let self,
                      event.windowNumber == windowNumber,
                      frameInWindow.contains(event.locationInWindow) else {
                    return event
                }
                let location = CGPoint(
                    x: event.locationInWindow.x - frameInWindow.minX,
                    y: frameInWindow.maxY - event.locationInWindow.y
                )

                onScroll(event.scrollingDeltaY, location)
                return nil
            }
        }

        func updateGeometry(windowNumber: Int, frameInWindow: CGRect) {
            self.windowNumber = windowNumber
            self.frameInWindow = frameInWindow
        }

        func removeMonitor() {
            if let monitor {
                NSEvent.removeMonitor(monitor)
                self.monitor = nil
            }
        }

        deinit {
            removeMonitor()
        }
    }

    final class ScrollCaptureView: NSView {
        weak var coordinator: Coordinator?

        override var isFlipped: Bool { true }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            syncGeometry()
        }

        override func layout() {
            super.layout()
            syncGeometry()
        }

        func syncGeometry() {
            guard let window else { return }
            coordinator?.updateGeometry(
                windowNumber: window.windowNumber,
                frameInWindow: convert(bounds, to: nil)
            )
        }
    }
}
