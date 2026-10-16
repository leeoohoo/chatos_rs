import AppKit
import Testing
@testable import ChatOSApp

@MainActor
@Suite("Pet overlay panel focus behavior")
struct PetOverlayPanelFactoryTests {
    @Test("Quick Chat remains visible without activating the foreground app")
    func quickChatPanelSupportsInputMethods() {
        let panel = PetOverlayPanelFactory.makePanel(
            size: NSSize(width: 420, height: 500),
            acceptsKeyboardInput: true
        )

        #expect(panel is PetMessagePanel)
        #expect(panel.canBecomeKey)
        #expect(panel.styleMask.contains(.nonactivatingPanel))
        #expect(!panel.becomesKeyOnlyIfNeeded)
        #expect(!panel.hidesOnDeactivate)
        #expect(panel.level == .floating)
    }

    @Test("Quick Chat requests key focus for focused text input")
    func quickChatRequestsKeyFocusForFocusedTextInput() async {
        let panel = PetOverlayPanelFactory.makePanel(
            size: NSSize(width: 420, height: 500),
            acceptsKeyboardInput: true
        )
        guard let panel = panel as? PetMessagePanel else {
            Issue.record("Expected a PetMessagePanel")
            return
        }
        let textView = NSTextView(frame: panel.contentView?.bounds ?? .zero)
        panel.contentView = textView

        await confirmation(expectedCount: 1) { confirm in
            panel.onTextInputFocusRequest = { responder in
                #expect(responder === textView)
                confirm()
            }
            #expect(panel.makeFirstResponder(textView))
            await withCheckedContinuation { continuation in
                DispatchQueue.main.async {
                    continuation.resume()
                }
            }
        }
    }

    @Test("Activating input panel joins the foreground application's window set")
    func activeInputPanelJoinsForegroundApplication() {
        let panel = PetOverlayPanelFactory.makeActiveInputPanel(
            size: NSSize(width: 420, height: 500)
        )

        #expect(panel.canBecomeKey)
        #expect(panel.canBecomeMain)
        #expect(!panel.styleMask.contains(.nonactivatingPanel))
        #expect(panel.collectionBehavior.contains(.canJoinAllSpaces))
        #expect(panel.collectionBehavior.contains(.canJoinAllApplications))
        #expect(panel.collectionBehavior.contains(.fullScreenAuxiliary))
        #expect(panel.collectionBehavior.contains(.transient))
        #expect(!panel.ignoresMouseEvents)
        #expect(panel.alphaValue == 1)
        #expect(panel.level == .floating)
    }

    @Test("Quick Chat recognizes clicks inside an already-focused text editor")
    func quickChatRecognizesTextInputHitTesting() {
        let panel = PetOverlayPanelFactory.makePanel(
            size: NSSize(width: 420, height: 500),
            acceptsKeyboardInput: true
        ) as! PetMessagePanel
        let textView = NSTextView(frame: NSRect(x: 20, y: 30, width: 240, height: 80))
        panel.contentView?.addSubview(textView)

        #expect(panel.textInputResponder(at: NSPoint(x: 40, y: 50)) === textView)
        #expect(panel.textInputResponder(at: NSPoint(x: 300, y: 300)) == nil)
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
