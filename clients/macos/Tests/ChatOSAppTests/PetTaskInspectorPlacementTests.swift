import AppKit
import Testing
@testable import ChatOSApp

@Suite("Pet task inspector placement")
struct PetTaskInspectorPlacementTests {
    @Test("places the inspector above the conversation when it fits")
    func placesAboveConversation() {
        let origin = PetTaskInspectorPlacement.origin(
            size: NSSize(width: 720, height: 620),
            conversationFrame: NSRect(x: 600, y: 180, width: 420, height: 500),
            petFrame: NSRect(x: 750, y: 60, width: 120, height: 120),
            visibleFrame: NSRect(x: 0, y: 0, width: 1800, height: 1400)
        )

        #expect(origin.y == 690)
        #expect(origin.x == 450)
    }

    @Test("places the inspector below the pet when the upper space is unavailable")
    func placesBelowPet() {
        let origin = PetTaskInspectorPlacement.origin(
            size: NSSize(width: 720, height: 620),
            conversationFrame: NSRect(x: 600, y: 500, width: 420, height: 500),
            petFrame: NSRect(x: 750, y: 360, width: 120, height: 120),
            visibleFrame: NSRect(x: 0, y: -500, width: 1800, height: 1700)
        )

        #expect(origin.y == -270)
        #expect(origin.x == 450)
    }
}
