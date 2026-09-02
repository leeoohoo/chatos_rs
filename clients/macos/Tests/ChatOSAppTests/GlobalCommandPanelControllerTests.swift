import AppKit
import Testing
@testable import ChatOSApp

@MainActor
struct GlobalCommandPanelControllerTests {
    @Test
    func globalPanelStaysVisibleAboveOtherApplications() {
        let controller = GlobalCommandPanelController(size: NSSize(width: 320, height: 180))
        let window = controller.window

        #expect(window?.hidesOnDeactivate == false)
        #expect(window?.level == .popUpMenu)
        #expect(window?.collectionBehavior.contains(.canJoinAllSpaces) == true)
        #expect(window?.collectionBehavior.contains(.fullScreenAuxiliary) == true)
    }

    @Test
    func globalPanelCanBecomeKeyForSearchInput() {
        let controller = GlobalCommandPanelController(size: NSSize(width: 320, height: 180))
        let panel = controller.window as? NSPanel

        #expect(panel?.canBecomeKey == true)
        #expect(panel?.becomesKeyOnlyIfNeeded == false)
    }
}
