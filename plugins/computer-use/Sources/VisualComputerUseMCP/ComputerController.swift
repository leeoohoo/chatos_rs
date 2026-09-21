@preconcurrency import AppKit
@preconcurrency import ApplicationServices
@preconcurrency import CoreGraphics
@preconcurrency import ImageIO
import Foundation
import UniformTypeIdentifiers

actor ComputerController {
    enum TextInputCommand: Sendable, Equatable {
        case unicode([UniChar])
        case key(CGKeyCode)
    }

    enum ScreenshotFormat: String, Sendable {
        case png
        case jpeg

        var mimeType: String {
            switch self {
            case .png: "image/png"
            case .jpeg: "image/jpeg"
            }
        }
    }

    struct ObservationOptions: Sendable {
        let displayID: UInt32?
        let region: RectDTO?
        let maxImageWidth: Int
        let format: ScreenshotFormat
        let jpegQuality: Double
    }

    struct CapturedObservation: Sendable {
        let metadata: ObservationDTO
        let imageData: Data
        let mimeType: String
    }

    private let eventSource = CGEventSource(stateID: .hidSystemState)
    private let iso8601 = ISO8601DateFormatter()
    private var virtualCursor: PointDTO?
    private var virtualCursorTrail: [PointDTO] = []
    private var cursorOverlay: VirtualCursorOverlay?

    func permissions(onboardingPresented: Bool = false) -> PermissionDTO {
        PermissionSupport.diagnostics(
            onboardingPresented: onboardingPresented
        )
    }

    func requestPermissions(
        screenRecording: Bool,
        accessibility: Bool
    ) async -> PermissionDTO {
        var requested = Set<MacPermissionKind>()
        if screenRecording { requested.insert(.screenRecording) }
        if accessibility { requested.insert(.accessibility) }
        guard !requested.isEmpty else {
            return permissions()
        }
        await PermissionOnboarding.present(requestedPermissions: requested)
        return permissions(onboardingPresented: true)
    }

    func activeApplication() async -> ActiveApplicationDTO {
        await MainActor.run {
            Self.applicationDTO(NSWorkspace.shared.frontmostApplication)
        }
    }

    func activateApplication(bundleIdentifier: String) async throws -> ActiveApplicationDTO {
        virtualCursorTrail = []
        return try await Self.activateApplicationOnMain(
            bundleIdentifier: bundleIdentifier
        )
    }

    @MainActor
    private static func activateApplicationOnMain(
        bundleIdentifier: String
    ) async throws -> ActiveApplicationDTO {
        if NSWorkspace.shared.frontmostApplication?.bundleIdentifier == bundleIdentifier {
            return applicationDTO(NSWorkspace.shared.frontmostApplication)
        }

        if let application = NSRunningApplication.runningApplications(
            withBundleIdentifier: bundleIdentifier
        ).first {
            var activationAccepted = false
            for _ in 0..<3 {
                if application.activate(options: [.activateAllWindows]) {
                    activationAccepted = true
                    break
                }
                try await Task.sleep(nanoseconds: 40_000_000)
            }
            guard activationAccepted else {
                throw VisualComputerUseError.applicationActivationFailed(bundleIdentifier)
            }
            return try await waitForFrontmost(
                application,
                bundleIdentifier: bundleIdentifier
            )
        }

        guard let url = NSWorkspace.shared.urlForApplication(
            withBundleIdentifier: bundleIdentifier
        ) else {
            throw VisualComputerUseError.applicationNotFound(bundleIdentifier)
        }

        let configuration = NSWorkspace.OpenConfiguration()
        configuration.activates = true
        let application: NSRunningApplication = try await withCheckedThrowingContinuation {
            continuation in
            NSWorkspace.shared.openApplication(at: url, configuration: configuration) {
                application, error in
                if let error {
                    continuation.resume(throwing: error)
                } else if let application {
                    continuation.resume(returning: application)
                } else {
                    continuation.resume(
                        throwing: VisualComputerUseError.applicationActivationFailed(
                            bundleIdentifier
                        )
                    )
                }
            }
        }
        return try await waitForFrontmost(
            application,
            bundleIdentifier: bundleIdentifier
        )
    }

    @MainActor
    private static func waitForFrontmost(
        _ application: NSRunningApplication,
        bundleIdentifier: String
    ) async throws -> ActiveApplicationDTO {
        for attempt in 0..<40 {
            if NSWorkspace.shared.frontmostApplication?.bundleIdentifier == bundleIdentifier {
                return applicationDTO(NSWorkspace.shared.frontmostApplication)
            }
            if application.isTerminated {
                break
            }
            if attempt > 0 && attempt.isMultiple(of: 8) {
                _ = application.activate(options: [.activateAllWindows])
            }
            try await Task.sleep(nanoseconds: 25_000_000)
        }
        throw VisualComputerUseError.applicationActivationFailed(bundleIdentifier)
    }

    func displays() throws -> [DisplayDTO] {
        var count: UInt32 = 0
        guard CGGetActiveDisplayList(0, nil, &count) == .success else {
            throw VisualComputerUseError.invalidArgument("Could not enumerate active displays.")
        }
        var ids = [CGDirectDisplayID](repeating: 0, count: Int(count))
        let result = ids.withUnsafeMutableBufferPointer { buffer in
            CGGetActiveDisplayList(count, buffer.baseAddress, &count)
        }
        guard result == .success else {
            throw VisualComputerUseError.invalidArgument("Could not enumerate active displays.")
        }
        return ids.prefix(Int(count)).map(Self.displayDTO)
    }

    func mousePosition() throws -> PointDTO {
        guard let event = CGEvent(source: nil) else {
            throw VisualComputerUseError.eventCreationFailed("mouse-position")
        }
        return PointDTO(x: event.location.x, y: event.location.y)
    }

    func observe(options: ObservationOptions) async throws -> CapturedObservation {
        guard CGPreflightScreenCaptureAccess() else {
            throw VisualComputerUseError.screenCapturePermissionRequired
        }

        let allDisplays = try displays()
        let cursor = try virtualCursorPosition(in: allDisplays)
        let selected: DisplayDTO
        if let displayID = options.displayID {
            guard let match = allDisplays.first(where: { $0.id == displayID }) else {
                throw VisualComputerUseError.displayNotFound(displayID)
            }
            selected = match
        } else if let region = options.region,
                  let containingDisplay = allDisplays.first(where: {
                      Self.contains($0.frame, region)
                  }) {
            selected = containingDisplay
        } else if let underCursor = allDisplays.first(where: { Self.contains($0.frame, cursor) }) {
            selected = underCursor
        } else if let main = allDisplays.first(where: \.isMain) ?? allDisplays.first {
            selected = main
        } else {
            throw VisualComputerUseError.invalidArgument("No active display is available.")
        }

        let captureRegion = options.region ?? selected.frame
        guard captureRegion.width > 0, captureRegion.height > 0 else {
            throw VisualComputerUseError.invalidArgument(
                "region width and height must be greater than zero."
            )
        }
        guard Self.contains(selected.frame, captureRegion) else {
            throw VisualComputerUseError.invalidArgument(
                "region must be fully contained within one active display."
            )
        }

        await showCursorOverlay(
            cursor: cursor,
            trail: virtualCursorTrail,
            displays: allDisplays
        )

        let displaySpaceRegion = CGRect(
            x: captureRegion.x - selected.frame.x,
            y: captureRegion.y - selected.frame.y,
            width: captureRegion.width,
            height: captureRegion.height
        )
        guard let captured = CGDisplayCreateImage(
            selected.id,
            rect: displaySpaceRegion
        ) else {
            throw VisualComputerUseError.screenCaptureFailed(selected.id)
        }

        let nativeWidth = max(
            1,
            Int((captureRegion.width * selected.nativePixelsPerPointX).rounded())
        )
        let targetWidth: Int
        if options.maxImageWidth > 0 {
            targetWidth = min(options.maxImageWidth, nativeWidth)
        } else {
            targetWidth = nativeWidth
        }
        let targetHeight = max(
            1,
            Int(
                (captureRegion.height * Double(targetWidth) / captureRegion.width)
                    .rounded()
            )
        )

        guard let rendered = Self.render(
            image: captured,
            width: targetWidth,
            height: targetHeight,
            cursor: cursor,
            cursorTrail: virtualCursorTrail,
            captureRegion: captureRegion,
            includeCursorMarker: true
        ) else {
            throw VisualComputerUseError.imageEncodingFailed
        }
        guard let imageData = Self.encodedData(
            from: rendered,
            format: options.format,
            jpegQuality: options.jpegQuality
        ) else {
            throw VisualComputerUseError.invalidEncodedImage
        }

        let foregroundApplication = await activeApplication()
        let metadata = ObservationDTO(
            coordinateSystem: "Global macOS display points and screenshot pixels both use a top-left origin; x grows right and y grows down. Screenshot pixel (imageX,imageY) maps to globalX = captureRegionGlobal.x + imageX * globalPointsPerScreenshotPixelX and globalY = captureRegionGlobal.y + imageY * globalPointsPerScreenshotPixelY. cursorScreenshotPixel is the cursor's direct image coordinate when visible. click uses virtualCursorGlobal.",
            activeApplication: foregroundApplication,
            globalDesktopBounds: Self.desktopBounds(allDisplays),
            selectedDisplay: selected,
            displays: allDisplays,
            captureRegionGlobal: captureRegion,
            virtualCursorGlobal: cursor,
            cursorScreenshotPixel: Self.screenshotPixel(
                for: cursor,
                captureRegion: captureRegion,
                screenshotWidth: targetWidth,
                screenshotHeight: targetHeight
            ),
            virtualCursorIsInCaptureRegion: Self.contains(captureRegion, cursor),
            cursorVisualization: Self.contains(captureRegion, cursor)
                ? "ai-orbit-reticle-with-cyan-hotspot"
                : "offscreen-edge-indicator",
            screenshotPixelWidth: targetWidth,
            screenshotPixelHeight: targetHeight,
            globalPointsPerScreenshotPixelX: captureRegion.width / Double(targetWidth),
            globalPointsPerScreenshotPixelY: captureRegion.height / Double(targetHeight),
            imageFormat: options.format.rawValue,
            encodedByteCount: imageData.count,
            cursorMarkerIncluded: true,
            capturedAt: iso8601.string(from: Date())
        )
        return CapturedObservation(
            metadata: metadata,
            imageData: imageData,
            mimeType: options.format.mimeType
        )
    }

    func moveVirtualCursor(
        to target: PointDTO,
        duration: Double,
        steps: Int
    ) async throws -> PointDTO {
        try validate(point: target)
        let allDisplays = try displays()
        let start = try virtualCursorPosition(in: allDisplays)
        let trajectory = Self.virtualTrajectory(
            from: start,
            to: target,
            steps: steps
        )
        let safeDuration = min(max(duration, 0), 3)
        for (index, point) in trajectory.enumerated() {
            virtualCursor = point
            virtualCursorTrail = Array(trajectory.prefix(index + 1))
            await showCursorOverlay(
                cursor: point,
                trail: virtualCursorTrail,
                displays: allDisplays
            )
            if safeDuration > 0 && index < trajectory.count - 1 {
                try await Task.sleep(
                    nanoseconds: UInt64(
                        safeDuration / Double(trajectory.count - 1)
                            * 1_000_000_000
                    )
                )
            }
        }
        return target
    }

    func currentVirtualCursor() throws -> PointDTO {
        try virtualCursorPosition(in: displays())
    }

    func click(button: String, count: Int, interval: Double) async throws -> PointDTO {
        try requireAccessibility()
        let point = try virtualCursorPosition(in: displays())
        let mapping = try Self.mouseButton(button)
        let safeCount = min(max(count, 1), 3)
        let safeInterval = min(max(interval, 0), 1)

        for clickIndex in 1...safeCount {
            guard let down = CGEvent(
                mouseEventSource: eventSource,
                mouseType: mapping.down,
                mouseCursorPosition: CGPoint(x: point.x, y: point.y),
                mouseButton: mapping.button
            ), let up = CGEvent(
                mouseEventSource: eventSource,
                mouseType: mapping.up,
                mouseCursorPosition: CGPoint(x: point.x, y: point.y),
                mouseButton: mapping.button
            ) else {
                throw VisualComputerUseError.eventCreationFailed("mouse-click")
            }
            down.setIntegerValueField(.mouseEventClickState, value: Int64(clickIndex))
            up.setIntegerValueField(.mouseEventClickState, value: Int64(clickIndex))
            down.post(tap: .cghidEventTap)
            up.post(tap: .cghidEventTap)

            if clickIndex < safeCount && safeInterval > 0 {
                try await Task.sleep(nanoseconds: UInt64(safeInterval * 1_000_000_000))
            }
        }
        virtualCursorTrail = []
        await showCursorOverlay(
            cursor: point,
            trail: [],
            displays: try displays()
        )
        return point
    }

    func scroll(
        deltaX: Int32,
        deltaY: Int32,
        duration: Double,
        steps: Int
    ) async throws {
        try requireAccessibility()
        let allDisplays = try displays()
        let point = try virtualCursorPosition(in: allDisplays)
        virtualCursorTrail = []
        await showCursorOverlay(
            cursor: point,
            trail: [],
            displays: allDisplays
        )
        let safeSteps = min(max(steps, 2), 80)
        let safeDuration = min(max(duration, 0), 3)
        let xDeltas = Self.smoothScrollDeltas(total: deltaX, steps: safeSteps)
        let yDeltas = Self.smoothScrollDeltas(total: deltaY, steps: safeSteps)

        for index in 0..<safeSteps {
            let stepX = xDeltas[index]
            let stepY = yDeltas[index]
            if stepX != 0 || stepY != 0 {
                guard let event = CGEvent(
                    scrollWheelEvent2Source: eventSource,
                    units: .pixel,
                    wheelCount: 2,
                    wheel1: stepY,
                    wheel2: stepX,
                    wheel3: 0
                ) else {
                    throw VisualComputerUseError.eventCreationFailed("scroll")
                }
                event.location = CGPoint(x: point.x, y: point.y)
                event.post(tap: .cghidEventTap)
            }

            if safeDuration > 0 && index < safeSteps - 1 {
                try await Task.sleep(
                    nanoseconds: UInt64(
                        safeDuration / Double(safeSteps - 1) * 1_000_000_000
                    )
                )
            }
        }
    }

    func typeText(_ text: String) throws {
        try requireAccessibility()
        virtualCursorTrail = []
        for command in Self.textInputCommands(text) {
            switch command {
            case .unicode(let units):
                try postUnicode(units)
            case .key(let keyCode):
                try postKey(keyCode)
            }
        }
    }

    static func textInputCommands(
        _ text: String,
        maxUnicodeUnits: Int = 24
    ) -> [TextInputCommand] {
        let safeLimit = max(1, maxUnicodeUnits)
        var commands: [TextInputCommand] = []
        var buffered: [UniChar] = []

        func flush() {
            guard !buffered.isEmpty else { return }
            commands.append(.unicode(buffered))
            buffered.removeAll(keepingCapacity: true)
        }

        for character in text {
            switch character {
            case "\n", "\r", "\r\n":
                flush()
                commands.append(.key(36))
            case "\t":
                flush()
                commands.append(.key(48))
            default:
                let units = Array(String(character).utf16)
                if !buffered.isEmpty && buffered.count + units.count > safeLimit {
                    flush()
                }
                buffered.append(contentsOf: units)
                if buffered.count >= safeLimit {
                    flush()
                }
            }
        }
        flush()
        return commands
    }

    private func postUnicode(_ units: [UniChar]) throws {
        guard !units.isEmpty else { return }
        guard let down = CGEvent(
            keyboardEventSource: eventSource,
            virtualKey: 0,
            keyDown: true
        ), let up = CGEvent(
            keyboardEventSource: eventSource,
            virtualKey: 0,
            keyDown: false
        ) else {
            throw VisualComputerUseError.eventCreationFailed("unicode-keyboard")
        }
        units.withUnsafeBufferPointer { buffer in
            down.keyboardSetUnicodeString(
                stringLength: buffer.count,
                unicodeString: buffer.baseAddress
            )
            up.keyboardSetUnicodeString(
                stringLength: buffer.count,
                unicodeString: buffer.baseAddress
            )
        }
        down.post(tap: .cghidEventTap)
        up.post(tap: .cghidEventTap)
    }

    private func postKey(_ keyCode: CGKeyCode) throws {
        guard let down = CGEvent(
            keyboardEventSource: eventSource,
            virtualKey: keyCode,
            keyDown: true
        ), let up = CGEvent(
            keyboardEventSource: eventSource,
            virtualKey: keyCode,
            keyDown: false
        ) else {
            throw VisualComputerUseError.eventCreationFailed("text-control-key")
        }
        down.post(tap: .cghidEventTap)
        up.post(tap: .cghidEventTap)
    }

    func wait(milliseconds: Int) async throws {
        let safeMilliseconds = min(max(milliseconds, 0), 5_000)
        if safeMilliseconds > 0 {
            try await Task.sleep(
                nanoseconds: UInt64(safeMilliseconds) * 1_000_000
            )
        }
    }

    func pressKeys(_ keys: [String]) throws {
        try requireAccessibility()
        virtualCursorTrail = []
        let shortcut = try Keyboard.parseShortcut(keys)
        guard let down = CGEvent(
            keyboardEventSource: eventSource,
            virtualKey: shortcut.keyCode,
            keyDown: true
        ), let up = CGEvent(
            keyboardEventSource: eventSource,
            virtualKey: shortcut.keyCode,
            keyDown: false
        ) else {
            throw VisualComputerUseError.eventCreationFailed("keyboard-shortcut")
        }
        down.flags = shortcut.flags
        up.flags = shortcut.flags
        down.post(tap: .cghidEventTap)
        up.post(tap: .cghidEventTap)
    }

    private func requireAccessibility() throws {
        guard AXIsProcessTrusted() else {
            throw VisualComputerUseError.accessibilityPermissionRequired
        }
    }

    private func validate(point: PointDTO) throws {
        let all = try displays()
        guard all.contains(where: { Self.contains($0.frame, point) }) else {
            throw VisualComputerUseError.pointOutsideDisplays(point.x, point.y)
        }
    }

    private func virtualCursorPosition(
        in allDisplays: [DisplayDTO]
    ) throws -> PointDTO {
        if let virtualCursor,
           allDisplays.contains(where: { Self.contains($0.frame, virtualCursor) }) {
            return virtualCursor
        }
        if let physical = try? mousePosition(),
           allDisplays.contains(where: { Self.contains($0.frame, physical) }) {
            virtualCursor = physical
            return physical
        }
        guard let display = allDisplays.first(where: \.isMain) ?? allDisplays.first else {
            throw VisualComputerUseError.invalidArgument(
                "No active display is available."
            )
        }
        let center = PointDTO(
            x: display.frame.x + display.frame.width / 2,
            y: display.frame.y + display.frame.height / 2
        )
        virtualCursor = center
        return center
    }

    private func showCursorOverlay(
        cursor: PointDTO,
        trail: [PointDTO],
        displays: [DisplayDTO]
    ) async {
        let overlay: VirtualCursorOverlay
        if let cursorOverlay {
            overlay = cursorOverlay
        } else {
            overlay = await MainActor.run { VirtualCursorOverlay() }
            cursorOverlay = overlay
        }
        await overlay.show(
            cursor: cursor,
            trail: trail,
            displays: displays
        )
    }

}
