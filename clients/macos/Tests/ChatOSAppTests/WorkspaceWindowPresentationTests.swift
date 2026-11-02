import AppKit
import XCTest
@testable import ChatOSApp

@MainActor
final class WorkspaceWindowPresentationTests: XCTestCase {
    func testEnteringFullScreenClearsActiveSpaceRelocationBeforeTransition() {
        let window = MainWorkspaceWindow(contentRect: .init(x: 0, y: 0, width: 900, height: 600),
                                         styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.collectionBehavior = [.fullScreenPrimary, .moveToActiveSpace]
        window.startPresentationDiagnostics()
        NotificationCenter.default.post(name: NSWindow.willEnterFullScreenNotification, object: window)
        XCTAssertFalse(window.collectionBehavior.contains(.moveToActiveSpace))
        XCTAssertTrue(window.collectionBehavior.contains(.fullScreenPrimary))
        window.close()
    }

    func testFullScreenReopenDoesNotMovePrimaryWindowToAnotherSpace() {
        let behavior = WorkspaceWindowPresentation.primaryBehavior(isFullScreen: true)
        XCTAssertTrue(behavior.contains(.fullScreenPrimary))
        XCTAssertFalse(behavior.contains(.moveToActiveSpace))
        XCTAssertFalse(behavior.contains(.canJoinAllSpaces))
        XCTAssertFalse(behavior.contains(.fullScreenAuxiliary))
    }

    func testReopenTargetsAttachedSheetInsteadOfMakingParentKey() {
        let parent = NSWindow(contentRect: .init(x: 0, y: 0, width: 900, height: 600),
                              styleMask: [.titled], backing: .buffered, defer: false)
        let sheet = NSWindow(contentRect: .init(x: 0, y: 0, width: 400, height: 250),
                             styleMask: [.titled], backing: .buffered, defer: false)
        parent.isReleasedWhenClosed = false
        sheet.isReleasedWhenClosed = false
        parent.beginSheet(sheet)
        XCTAssertTrue(WorkspaceWindowPresentation.focusTarget(for: parent) === sheet)
        parent.endSheet(sheet)
        sheet.close()
        parent.close()
    }
}
