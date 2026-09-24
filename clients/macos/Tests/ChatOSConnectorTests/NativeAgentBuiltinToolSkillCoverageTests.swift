@testable import ChatOSConnector
import ChatOSCore
import XCTest

final class NativeAgentBuiltinToolSkillCoverageTests: XCTestCase {
    func testNativeBuiltinAuditEnumeratesLiveDefinitionsAndCoversTerminalFamily() throws {
        let report = try NativeAgentBuiltinToolProvider.skillCoverageReport()

        XCTAssertGreaterThan(report.totalTools, 9)
        XCTAssertEqual(report.coveredTools, 9)
        XCTAssertEqual(report.issues.count, report.totalTools - report.coveredTools)
        XCTAssertTrue(report.issues.allSatisfy { issue in
            issue.kind == .missingBindingID
                && issue.providerID != ProductToolProviderID.terminal
        })
    }
}
