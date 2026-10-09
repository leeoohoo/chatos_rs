import ChatOSCore
import XCTest
@testable import ChatOSApp

@MainActor
final class AgentManagementCardTests: XCTestCase {
    func testPreparingOneEditorOnlyInvalidatesItsOwnCard() {
        let ids = (1...12).map { "agent-\($0)" }
        let before = ids.map { card(id: $0) }
        let after = ids.map { card(id: $0, preparingID: "agent-5") }
        let changed = zip(before, after).filter { $0 != $1 }.map { $0.0.agent.id }
        XCTAssertEqual(changed, ["agent-5"])
    }

    func testMeaningfulModelSelectionAndProfessionChangesStillInvalidateCard() {
        let baseline = card(id: "agent")
        XCTAssertNotEqual(baseline, card(id: "agent", availability: .unavailable))
        XCTAssertNotEqual(baseline, card(id: "agent", selected: true))
        XCTAssertNotEqual(baseline, card(id: "agent", profession: "Backend Engineer"))
        XCTAssertEqual(baseline, card(id: "agent"), "Recreated action closures must not redraw unchanged cards")
    }

    private func card(
        id: String, preparingID: String? = nil,
        availability: AgentModelAvailability = .unchecked,
        selected: Bool = false, profession: String = "General"
    ) -> AgentManagementCard {
        .init(
            agent: .init(id: id, ownerUserID: "alice", draft: .init(
                name: id, rolePrompt: "Test", modelConfigID: "model"
            ), createdAtUnixMs: 1, updatedAtUnixMs: 1),
            modelAvailability: availability, professionLabel: profession,
            isSelected: selected, isPreparingEditor: preparingID == id,
            onOpenDirect: {}, onEdit: {}, onSelect: {}
        )
    }
}
