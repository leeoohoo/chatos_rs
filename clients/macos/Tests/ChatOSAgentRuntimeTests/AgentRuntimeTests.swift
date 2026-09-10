import Foundation
import XCTest
@testable import ChatOSAgentRuntime

final class AgentRuntimeTests: XCTestCase {
    func testDefaultBudgetIs600() throws {
        let settings = AgentRuntimePreferences()
        XCTAssertEqual(settings.effective(.story).maximumModelCalls, 600)
        XCTAssertEqual(settings.effective(.approval).maximumModelCalls, 600)
        try settings.validate()
    }
}
