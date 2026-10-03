@testable import ChatOSConnector
import Foundation
import XCTest

final class NativeLocalAgentAsyncUtilitiesTests: XCTestCase {
    func testRequiredOptionalLoaderStartsBothRequestsConcurrently() async throws {
        let gate = ParallelLoaderGate()
        let load = Task {
            try await NativeRequiredOptionalParallelLoader.load {
                await gate.arriveAndWait()
                return "required"
            } optional: {
                await gate.arriveAndWait()
                return "optional"
            }
        }

        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: .seconds(1))
        while await gate.arrivalCount < 2, clock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        let arrivalCount = await gate.arrivalCount
        XCTAssertEqual(arrivalCount, 2)
        await gate.release()

        let result = try await load.value
        XCTAssertEqual(result.required, "required")
        XCTAssertEqual(result.optional, "optional")
    }

    func testRequiredOptionalLoaderKeepsOptionalFailureNonFatal() async throws {
        let result: (required: Int, optional: String?) = try await NativeRequiredOptionalParallelLoader.load {
            42
        } optional: {
            throw AsyncLoaderTestError.expected
        }

        XCTAssertEqual(result.required, 42)
        XCTAssertNil(result.optional)
    }

    func testRequiredOptionalLoaderPropagatesRequiredFailure() async {
        do {
            let _: (required: Int, optional: String?) = try await NativeRequiredOptionalParallelLoader.load {
                throw AsyncLoaderTestError.expected
            } optional: {
                "optional"
            }
            XCTFail("Expected the required request failure to propagate")
        } catch {
            XCTAssertEqual(error as? AsyncLoaderTestError, .expected)
        }
    }

    func testBoundedLoaderPreservesInputOrderAndConcurrencyLimit() async throws {
        let probe = AsyncLoaderProbe()
        let values = try await NativeLocalAgentBoundedLoader.load(
            Array(0..<12),
            maximumConcurrentTasks: 4
        ) { value in
            await probe.begin()
            try await Task.sleep(for: .milliseconds(Int64((12 - value) * 2)))
            await probe.end()
            return value * 10
        }

        XCTAssertEqual(values, Array(0..<12).map { $0 * 10 })
        let maximumConcurrency = await probe.maximumConcurrency
        XCTAssertEqual(maximumConcurrency, 4)
    }

    func testEventWaitUsesHostMaximumWithinProductionRequestDeadline() {
        XCTAssertEqual(NativeLocalAgentEventWaitPolicy.timeoutMilliseconds, 60_000)
        XCTAssertLessThan(NativeLocalAgentEventWaitPolicy.timeoutMilliseconds, 75_000)
    }
}

private enum AsyncLoaderTestError: Error {
    case expected
}

private actor ParallelLoaderGate {
    private(set) var arrivalCount = 0
    private var released = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func arriveAndWait() async {
        arrivalCount += 1
        guard !released else { return }
        await withCheckedContinuation { waiters.append($0) }
    }

    func release() {
        released = true
        let pending = waiters
        waiters.removeAll(keepingCapacity: false)
        pending.forEach { $0.resume() }
    }
}

private actor AsyncLoaderProbe {
    private var currentConcurrency = 0
    private(set) var maximumConcurrency = 0

    func begin() {
        currentConcurrency += 1
        maximumConcurrency = max(maximumConcurrency, currentConcurrency)
    }

    func end() {
        currentConcurrency -= 1
    }
}
