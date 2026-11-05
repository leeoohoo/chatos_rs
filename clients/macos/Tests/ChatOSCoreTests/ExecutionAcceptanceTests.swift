import ChatOSCore
import XCTest

final class ExecutionAcceptanceTests: XCTestCase {
    private func check(
        _ index: Int,
        _ status: AgentExecutionAcceptanceStatus,
        requiresHuman: Bool = false
    ) -> AgentExecutionAcceptanceGateCheck {
        .init(criterionIndex: index, status: status, requiresHuman: requiresHuman)
    }

    func testCompletionRequiresEveryCheckToPass() {
        XCTAssertTrue(AgentExecutionAcceptanceGate.canComplete([
            check(0, .passed),
            check(1, .passed),
        ]))
        XCTAssertFalse(AgentExecutionAcceptanceGate.canComplete([]))
        XCTAssertFalse(AgentExecutionAcceptanceGate.canComplete([
            check(0, .passed),
            check(1, .notVerified),
        ]))
    }

    func testHumanBlockIsTerminalOnlyAfterOtherChecksPass() {
        XCTAssertTrue(AgentExecutionAcceptanceGate.canFinishBlocked([
            check(0, .blocked, requiresHuman: true),
            check(1, .passed),
        ]))

        for status in [
            AgentExecutionAcceptanceStatus.pending,
            .inProgress,
            .failed,
            .notVerified,
        ] {
            let checks = [
                check(0, .blocked, requiresHuman: true),
                check(1, status),
            ]
            XCTAssertFalse(
                AgentExecutionAcceptanceGate.canFinishBlocked(checks),
                "Human blocking one criterion must not waive a \(status.rawValue) criterion"
            )
            XCTAssertEqual(
                AgentExecutionAcceptanceGate.actionableChecks(checks).map(\.criterionIndex),
                [1]
            )
        }
    }

    func testNonHumanBlockRemainsActionable() {
        let checks = [
            check(0, .blocked),
            check(1, .passed),
        ]
        XCTAssertFalse(AgentExecutionAcceptanceGate.canFinishBlocked(checks))
        XCTAssertEqual(
            AgentExecutionAcceptanceGate.actionableChecks(checks).map(\.criterionIndex),
            [0]
        )
    }
}
