@testable import ChatOSConnector
import Foundation
import XCTest

final class NativeLocalAgentAsyncUtilitiesTests: XCTestCase {
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

    func testEventPollingBacksOffAndCapsAtTwoSeconds() {
        var delay = NativeLocalAgentEventPollingPolicy.activeDelay
        delay = NativeLocalAgentEventPollingPolicy.nextIdleDelay(after: delay)
        XCTAssertEqual(delay, .milliseconds(500))
        delay = NativeLocalAgentEventPollingPolicy.nextIdleDelay(after: delay)
        XCTAssertEqual(delay, .seconds(1))
        delay = NativeLocalAgentEventPollingPolicy.nextIdleDelay(after: delay)
        XCTAssertEqual(delay, .seconds(2))
        XCTAssertEqual(
            NativeLocalAgentEventPollingPolicy.nextIdleDelay(after: delay),
            .seconds(2)
        )
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
