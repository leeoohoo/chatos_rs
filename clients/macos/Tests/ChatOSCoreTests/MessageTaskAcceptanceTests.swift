import ChatOSCore
import Foundation
import XCTest

final class MessageTaskAcceptanceTests: XCTestCase {
    private func report(_ status: String, _ verdicts: [String]) throws -> MessageTaskAcceptanceReport {
        let data = try JSONSerialization.data(withJSONObject: ["status": status, "reason": "Concrete outcome",
            "acceptance_checks": verdicts.enumerated().map { ["criterion_index": $0.offset, "status": $0.element, "evidence": "Observed criterion \($0.offset)"] }])
        return try XCTUnwrap(MessageTaskAcceptanceReport.decode(json: String(decoding: data, as: UTF8.self)))
    }

    func testAcceptanceIsSeparateFromExecutionAndPreservesTaskFailure() throws {
        let partial = try report("failed", ["passed", "not_verified"])
        var task = MessageTask(id: "task", title: "Research", status: "failed")
        task = task.merging(run: MessageTaskRun(id: "run", taskID: "task", status: "succeeded", acceptanceReport: partial))
        XCTAssertEqual(task.resultDisposition, .evidenceIncomplete)
        XCTAssertEqual(task.status, "failed", "UI must not change dependency/scheduling truth")
        XCTAssertEqual(task.lastRun?.acceptanceReport?.checks.count, 2)
        task = task.merging(run: MessageTaskRun(id: "run", taskID: "task", status: "failed", acceptanceReport: partial))
        XCTAssertEqual(task.resultDisposition, .executionFailed, "Execution error must not be disguised by partial evidence")
        XCTAssertEqual(try report("failed", ["passed", "failed"]).disposition, .acceptanceFailed)
        XCTAssertEqual(try report("blocked", ["not_verified"]).disposition, .blocked)
        XCTAssertEqual(try report("succeeded", ["passed"]).disposition, .acceptancePassed)
    }

    func testMalformedAndMissingReportsCannotImplyAcceptance() {
        for json in ["{}", "not json", #"{"status":"succeeded","reason":"done","acceptance_checks":[{"criterion_index":0,"status":"not_verified","evidence":"missing"}]}"#,
            #"{"status":"succeeded","reason":"done","acceptance_checks":[]}"#,
            #"{"status":"failed","reason":"gap","acceptance_checks":[{"criterion_index":5,"status":"not_verified","evidence":"missing"}]}"#] {
            XCTAssertNil(MessageTaskAcceptanceReport.decode(json: json))
        }
        let task = MessageTask(id: "t", title: "Unknown outcome", status: "failed")
        XCTAssertNil(task.resultDisposition, "Do not guess from unstructured model prose")
    }
}
