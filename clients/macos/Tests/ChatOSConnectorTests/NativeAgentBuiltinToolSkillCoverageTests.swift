@testable import ChatOSConnector
import ChatOSCore
import XCTest

final class NativeAgentBuiltinToolSkillCoverageTests: XCTestCase {
    func testNativeBuiltinAuditEnumeratesLiveDefinitionsAndIsComplete() throws {
        let report = try NativeAgentBuiltinToolProvider.skillCoverageReport()

        XCTAssertEqual(report.totalTools, 28)
        XCTAssertEqual(report.coveredTools, report.totalTools)
        XCTAssertTrue(report.isComplete)
        XCTAssertTrue(report.issues.isEmpty)
    }
}
