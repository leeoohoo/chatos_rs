import CoreGraphics
import Testing
@testable import VisualComputerUseMCP

@Test func parsesKeyboardShortcut() throws {
    let shortcut = try Keyboard.parseShortcut(["Command", "Shift", "P"])
    #expect(shortcut.keyCode == 35)
    #expect(shortcut.flags.contains(.maskCommand))
    #expect(shortcut.flags.contains(.maskShift))
}

@Test func rejectsMultipleNonModifierKeys() {
    #expect(throws: VisualComputerUseError.self) {
        _ = try Keyboard.parseShortcut(["command", "a", "b"])
    }
}

@Test func textInputCommandsPreserveUnicodeAndControlKeys() {
    let commands = ComputerController.textInputCommands(
        "设计😀\n下一行\t完成",
        maxUnicodeUnits: 6
    )
    #expect(commands.contains(.key(36)))
    #expect(commands.contains(.key(48)))
    let unicode = commands.compactMap { command -> [UniChar]? in
        guard case .unicode(let units) = command else { return nil }
        return units
    }.flatMap { $0 }
    #expect(String(decoding: unicode, as: UTF16.self) == "设计😀下一行完成")
    #expect(commands.allSatisfy { command in
        guard case .unicode(let units) = command else { return true }
        return units.count <= 6
    })
}

@Test func displayContainmentUsesTopLeftGlobalPoints() {
    let rect = RectDTO(x: -1920, y: 0, width: 1920, height: 1080)
    #expect(ComputerController.contains(rect, PointDTO(x: -1, y: 1079)))
    #expect(!ComputerController.contains(rect, PointDTO(x: 0, y: 500)))
}

@Test func rectangleContainmentRequiresFullPositiveRegion() {
    let display = RectDTO(x: -100, y: 20, width: 800, height: 600)
    #expect(ComputerController.contains(
        display,
        RectDTO(x: -100, y: 20, width: 800, height: 600)
    ))
    #expect(ComputerController.contains(
        display,
        RectDTO(x: 0, y: 100, width: 320, height: 240)
    ))
    #expect(!ComputerController.contains(
        display,
        RectDTO(x: 650, y: 100, width: 100, height: 100)
    ))
    #expect(!ComputerController.contains(
        display,
        RectDTO(x: 0, y: 100, width: 0, height: 100)
    ))
}

@Test func virtualCursorTrajectoryHasStableEndpointsAndCurve() {
    let start = PointDTO(x: 100, y: 200)
    let target = PointDTO(x: 900, y: 600)
    let points = ComputerController.virtualTrajectory(
        from: start,
        to: target,
        steps: 28
    )
    #expect(points.count == 28)
    #expect(points.first == start)
    #expect(points.last == target)
    #expect(points.dropFirst().dropLast().contains { point in
        let expectedY = start.y
            + (point.x - start.x) * (target.y - start.y) / (target.x - start.x)
        return abs(point.y - expectedY) > 1
    })
}

@Test func desktopBoundsUnionSupportsNegativeDisplayCoordinates() {
    let displays = [
        DisplayDTO(
            id: 1,
            isMain: true,
            frame: RectDTO(x: 0, y: 0, width: 2560, height: 1600),
            nativePixelWidth: 2560,
            nativePixelHeight: 1600,
            nativePixelsPerPointX: 1,
            nativePixelsPerPointY: 1
        ),
        DisplayDTO(
            id: 2,
            isMain: false,
            frame: RectDTO(x: -1920, y: 100, width: 1920, height: 1080),
            nativePixelWidth: 1920,
            nativePixelHeight: 1080,
            nativePixelsPerPointX: 1,
            nativePixelsPerPointY: 1
        )
    ]
    #expect(
        ComputerController.desktopBounds(displays)
            == RectDTO(x: -1920, y: 0, width: 4480, height: 1600)
    )
}

@Test func smoothScrollDeltasPreserveTotalAndEaseMovement() {
    let positive = ComputerController.smoothScrollDeltas(total: 240, steps: 20)
    let negative = ComputerController.smoothScrollDeltas(total: -37, steps: 18)

    #expect(positive.count == 20)
    #expect(positive.reduce(Int32(0), +) == 240)
    #expect(negative.count == 18)
    #expect(negative.reduce(Int32(0), +) == -37)
    #expect(abs(positive[positive.count / 2]) > abs(positive[0]))
    #expect(abs(positive[positive.count / 2]) > abs(positive[positive.count - 1]))
}

@Test func scrollDeltaValidationUsesBoundedPixelDistances() throws {
    #expect(try MCPService.validatedScrollDeltaPixels(-250, name: "delta_y") == -250)
    #expect(try MCPService.validatedScrollDeltaPixels(1_200, name: "delta_y") == 1_200)
    #expect(throws: VisualComputerUseError.self) {
        _ = try MCPService.validatedScrollDeltaPixels(1_201, name: "delta_y")
    }
    #expect(throws: VisualComputerUseError.self) {
        _ = try MCPService.validatedScrollDeltaPixels(-1_201, name: "delta_x")
    }
}

@Test func cursorScreenshotPixelUsesCropLocalTopLeftCoordinates() {
    let pixel = ComputerController.screenshotPixel(
        for: PointDTO(x: 1600, y: 500),
        captureRegion: RectDTO(x: 1280, y: 100, width: 1280, height: 800),
        screenshotWidth: 640,
        screenshotHeight: 400
    )
    #expect(pixel == PointDTO(x: 160, y: 200))
    #expect(ComputerController.screenshotPixel(
        for: PointDTO(x: 900, y: 500),
        captureRegion: RectDTO(x: 1280, y: 100, width: 1280, height: 800),
        screenshotWidth: 640,
        screenshotHeight: 400
    ) == nil)
}

@Test func permissionDiagnosticsExposeBothMacSettingsDestinations() {
    let diagnostics = PermissionSupport.diagnostics()
    #expect(diagnostics.permissions.count == 2)
    #expect(!diagnostics.authorizationTarget.isEmpty)
    #expect(diagnostics.permissions.contains {
        $0.kind == MacPermissionKind.accessibility.rawValue
            && $0.settingsURL.contains("Privacy_Accessibility")
    })
    #expect(diagnostics.permissions.contains {
        $0.kind == MacPermissionKind.screenRecording.rawValue
            && $0.settingsURL.contains("Privacy_ScreenCapture")
    })
}
