import CoreGraphics
import Foundation
import Testing
@testable import VisualComputerUseMCP

struct ActiveApplicationTests {
    @Test("frontmost window title uses the first visible standard window for the process")
    func frontmostWindowTitleSelection() {
        let windows: [[String: Any]] = [
            [
                kCGWindowOwnerPID as String: NSNumber(value: 99),
                kCGWindowLayer as String: NSNumber(value: 0),
                kCGWindowName as String: "Other App",
            ],
            [
                kCGWindowOwnerPID as String: NSNumber(value: 42),
                kCGWindowLayer as String: NSNumber(value: 1),
                kCGWindowName as String: "Overlay",
            ],
            [
                kCGWindowOwnerPID as String: NSNumber(value: 42),
                kCGWindowLayer as String: NSNumber(value: 0),
                kCGWindowName as String: "  插件验收-20261014  ",
            ],
            [
                kCGWindowOwnerPID as String: NSNumber(value: 42),
                kCGWindowLayer as String: NSNumber(value: 0),
                kCGWindowName as String: "Second Window",
            ],
        ]

        #expect(ComputerController.frontmostWindowTitle(
            processIdentifier: 42,
            windowInfo: windows
        ) == "插件验收-20261014")
    }

    @Test("frontmost window title ignores empty and unrelated windows")
    func frontmostWindowTitleMissing() {
        let windows: [[String: Any]] = [
            [
                kCGWindowOwnerPID as String: NSNumber(value: 42),
                kCGWindowLayer as String: NSNumber(value: 0),
                kCGWindowName as String: "   ",
            ],
            [
                kCGWindowOwnerPID as String: NSNumber(value: 7),
                kCGWindowLayer as String: NSNumber(value: 0),
                kCGWindowName as String: "Unrelated",
            ],
        ]

        #expect(ComputerController.frontmostWindowTitle(
            processIdentifier: 42,
            windowInfo: windows
        ) == nil)
    }

    @Test("running applications match exact user-visible identities")
    func runningApplicationNameMatching() {
        #expect(ComputerController.applicationMatches(
            "QQ音乐",
            name: "QQ音乐",
            bundleIdentifier: "com.tencent.QQMusicMac",
            executableName: "QQMusic",
            bundleName: "QQMusic.app"
        ))
        #expect(ComputerController.applicationMatches(
            "qqmusic.app",
            name: "QQ音乐",
            bundleIdentifier: "com.tencent.QQMusicMac",
            executableName: "QQMusic",
            bundleName: "QQMusic.app"
        ))
        #expect(ComputerController.applicationMatches(
            "com.tencent.QQMusicMac",
            name: "QQ音乐",
            bundleIdentifier: "com.tencent.QQMusicMac",
            executableName: "QQMusic",
            bundleName: "QQMusic.app"
        ))
        #expect(!ComputerController.applicationMatches(
            "QQ",
            name: "QQ音乐",
            bundleIdentifier: "com.tencent.QQMusicMac",
            executableName: "QQMusic",
            bundleName: "QQMusic.app"
        ))
    }
}
