@testable import ChatOSApp
import XCTest

@MainActor
final class AgentChangeRefreshCoalescerTests: XCTestCase {
    func testBurstSignalsProduceOneRefresh() async throws {
        var refreshCount = 0
        let coalescer = AgentChangeRefreshCoalescer(delay: .milliseconds(10)) {
            refreshCount += 1
        }

        for _ in 0..<500 {
            coalescer.signal()
        }
        try await Task.sleep(for: .milliseconds(80))

        XCTAssertEqual(refreshCount, 1)
        coalescer.cancel()
    }

    func testChangeDuringRefreshSchedulesOneFollowUp() async throws {
        let firstRefreshStarted = expectation(description: "first refresh started")
        var refreshCount = 0
        let coalescer = AgentChangeRefreshCoalescer(delay: .milliseconds(10)) {
            refreshCount += 1
            if refreshCount == 1 {
                firstRefreshStarted.fulfill()
                try? await Task.sleep(for: .milliseconds(40))
            }
        }

        coalescer.signal()
        await fulfillment(of: [firstRefreshStarted], timeout: 1)
        coalescer.signal()
        try await Task.sleep(for: .milliseconds(120))

        XCTAssertEqual(refreshCount, 2)
        coalescer.cancel()
    }
}
