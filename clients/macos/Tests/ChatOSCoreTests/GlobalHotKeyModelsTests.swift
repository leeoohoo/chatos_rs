import Foundation
import Testing
@testable import ChatOSCore

struct GlobalHotKeyModelsTests {
    @Test
    func defaultsMatchRequestedGlobalShortcuts() {
        #expect(GlobalUtilityAction.screenshot.defaultHotKey.displayName == "⌃A")
        #expect(GlobalUtilityAction.screenRecording.defaultHotKey.displayName == "⌃Q")
        #expect(GlobalUtilityAction.clipboardHistory.defaultHotKey.displayName == "⌘E")
        #expect(GlobalUtilityAction.quickSearch.defaultHotKey.displayName == "⌘Space")
        #expect(GlobalUtilityAction.quickSearch.fallbackHotKey?.displayName == "⌥Space")
    }

    @Test
    func hotKeyRoundTripsThroughCodableStorage() throws {
        let source = GlobalHotKey(
            keyCode: 1,
            keyEquivalent: "S",
            modifiers: [.command, .shift]
        )
        let encoded = try JSONEncoder().encode(source)
        let decoded = try JSONDecoder().decode(GlobalHotKey.self, from: encoded)

        #expect(decoded == source)
        #expect(decoded.displayName == "⇧⌘S")
    }
}
