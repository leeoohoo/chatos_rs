import ChatOSCore
import XCTest
@testable import ChatOSApp

final class LocalConnectorCustomMCPExamplesTests: XCTestCase {
    func testREAExampleUsesDeveloperAnalysisProfile() {
        let example = LocalConnectorCustomMCPExamples.rea

        XCTAssertEqual(example.packageName, "rea-agents")
        XCTAssertEqual(example.binaryName, "rea")
        XCTAssertEqual(example.arguments, ["mcp"])
        XCTAssertEqual(example.accessProfile, .developerAnalysis)
    }
}
