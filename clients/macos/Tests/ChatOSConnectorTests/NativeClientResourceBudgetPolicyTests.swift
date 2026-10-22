@testable import ChatOSConnector
import XCTest

final class NativeClientResourceBudgetPolicyTests: XCTestCase {
    func testAdaptiveBudgetSplitsOneTotalAcrossHostAndSwiftAgent() {
        let budget = NativeClientResourceBudgetPolicy.recommended(logicalProcessorCount: 12)

        XCTAssertEqual(budget.totalLogicalJobs, 3)
        XCTAssertEqual(budget.localAgentHostWorkItems, 1)
        XCTAssertEqual(budget.swiftAgentRuns, 2)
        XCTAssertEqual(
            budget.localAgentHostWorkItems + budget.swiftAgentRuns,
            budget.totalLogicalJobs
        )
    }

    func testConfiguredHostCannotConsumeSwiftAgentReserve() {
        let budget = NativeClientResourceBudgetPolicy.recommended(
            logicalProcessorCount: 12,
            configuredTotalLogicalJobs: 5,
            configuredHostWorkItems: 16
        )

        XCTAssertEqual(budget.totalLogicalJobs, 5)
        XCTAssertEqual(budget.localAgentHostWorkItems, 4)
        XCTAssertEqual(budget.swiftAgentRuns, 1)
    }

    func testInvalidConfigurationFallsBackToAdaptiveBounds() {
        XCTAssertNil(NativeClientResourceBudgetPolicy.validatedTotal(1))
        XCTAssertNil(NativeClientResourceBudgetPolicy.validatedTotal(17))
        XCTAssertEqual(
            NativeClientResourceBudgetPolicy.recommended(
                logicalProcessorCount: 2,
                configuredTotalLogicalJobs: 100
            ).totalLogicalJobs,
            2
        )
        XCTAssertEqual(
            NativeClientResourceBudgetPolicy.recommended(
                logicalProcessorCount: 128
            ).totalLogicalJobs,
            3
        )
    }

    func testExecutionGovernorCancellationReturnsQueueCapacity() async throws {
        let governor = LocalAgentExecutionGovernor(limit: 1)
        let firstValue = await governor.acquire()
        let first = try XCTUnwrap(firstValue)
        let waiting = Task { await governor.acquire() }
        await Task.yield()

        waiting.cancel()
        let cancelledPermit = await waiting.value
        XCTAssertNil(cancelledPermit)

        await first.releaseAndWait()
        let replacementValue = await governor.acquire()
        let replacement = try XCTUnwrap(replacementValue)
        let overBudget = await governor.tryAcquire()
        XCTAssertNil(overBudget)
        await replacement.releaseAndWait()
    }
}
