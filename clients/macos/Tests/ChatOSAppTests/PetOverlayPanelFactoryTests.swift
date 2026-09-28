import AppKit
import Testing
@testable import ChatOSApp

@MainActor
@Suite("Pet overlay panel focus behavior")
struct PetOverlayPanelFactoryTests {
    @Test("Quick Chat uses an activating key-capable panel for input methods")
    func quickChatPanelSupportsInputMethods() {
        let panel = PetOverlayPanelFactory.makePanel(
            size: NSSize(width: 420, height: 500),
            acceptsKeyboardInput: true
        )

        #expect(panel is PetMessagePanel)
        #expect(panel.canBecomeKey)
        #expect(!panel.styleMask.contains(.nonactivatingPanel))
        #expect(!panel.becomesKeyOnlyIfNeeded)
    }

    @Test("The pet panel remains passive")
    func passivePanelDoesNotActivateTheApp() {
        let panel = PetOverlayPanelFactory.makePanel(
            size: NSSize(width: 160, height: 160)
        )

        #expect(panel.styleMask.contains(.nonactivatingPanel))
        #expect(panel.becomesKeyOnlyIfNeeded)
    }
}
