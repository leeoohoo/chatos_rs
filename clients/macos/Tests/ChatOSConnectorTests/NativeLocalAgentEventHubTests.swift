@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentEventHubTests: XCTestCase {
    func testMultipleSubscribersShareOnePollingLoopAndLastCancellationStopsIt() async throws {
        let host = EventHubHostStub(listDelay: .milliseconds(150))
        let hub = NativeLocalAgentEventHub(host: host)
        await hub.configure(ownerUserID: "user-1")

        let firstStream = await hub.updates()
        let secondStream = await hub.updates()
        let firstConsumer = Task {
            for await _ in firstStream {
                if Task.isCancelled { return }
            }
        }
        let secondConsumer = Task {
            for await _ in secondStream {
                if Task.isCancelled { return }
            }
        }

        try await waitUntil { await host.listEventRequestCount() >= 1 }
        try await Task.sleep(for: .milliseconds(250))
        let maximumConcurrentListRequests = await host.maximumConcurrentListRequests()
        XCTAssertEqual(maximumConcurrentListRequests, 1)

        firstConsumer.cancel()
        secondConsumer.cancel()
        _ = await firstConsumer.result
        _ = await secondConsumer.result
        try await waitUntil { await host.activeListRequestCount() == 0 }
        let cancelledWaitRequests = await host.cancelledWaitRequestCount()
        XCTAssertEqual(cancelledWaitRequests, 0)
        let requestCountAfterCancellation = await host.listEventRequestCount()
        try await Task.sleep(for: .milliseconds(400))
        let finalRequestCount = await host.listEventRequestCount()
        XCTAssertEqual(finalRequestCount, requestCountAfterCancellation)
    }

    func testOwnerSwitchDiscardsInFlightEventsFromPreviousGeneration() async throws {
        let host = EventHubHostStub(
            listDelay: .milliseconds(150),
            eventOwners: ["user-1", "user-2"]
        )
        let hub = NativeLocalAgentEventHub(host: host)
        await hub.configure(ownerUserID: "user-1")
        let stream = await hub.updates()
        var iterator = stream.makeAsyncIterator()

        let initialValue = await iterator.next()
        let initial = try XCTUnwrap(initialValue)
        XCTAssertEqual(initial.ownerUserID, "user-1")
        try await waitUntil { await host.listEventRequestCount(ownerUserID: "user-1") >= 1 }

        await hub.configure(ownerUserID: "user-2")
        let switchedValue = await iterator.next()
        let switched = try XCTUnwrap(switchedValue)
        XCTAssertEqual(switched.ownerUserID, "user-2")
        guard case .reconcile = switched.kind else {
            return XCTFail("Expected an owner-switch reconciliation")
        }

        let pageValue = await iterator.next()
        let page = try XCTUnwrap(pageValue)
        XCTAssertEqual(page.ownerUserID, "user-2")
        guard case let .events(events) = page.kind else {
            return XCTFail("Expected the new owner's event page")
        }
        XCTAssertEqual(events.map(\.runID), ["run-user-2"])
    }

    private func waitUntil(
        timeout: Duration = .seconds(3),
        condition: @escaping @Sendable () async -> Bool
    ) async throws {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: timeout)
        while !(await condition()) {
            guard clock.now < deadline else {
                throw CocoaError(.coderReadCorrupt)
            }
            try await Task.sleep(for: .milliseconds(10))
        }
    }
}

private actor EventHubHostStub: LocalAgentHostClientServicing {
    private let listDelay: Duration
    private let eventOwners: Set<String>
    private var deliveredEventOwners = Set<String>()
    private var listRequestsByOwner: [String: Int] = [:]
    private var activeListRequests = 0
    private var maximumConcurrentListRequestCount = 0
    private var cancelledWaitRequests = 0

    init(listDelay: Duration, eventOwners: Set<String> = []) {
        self.listDelay = listDelay
        self.eventOwners = eventOwners
    }

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        let type = object?["type"] as? String
        let ownerUserID = object?["owner_user_id"] as? String ?? ""
        if type == "get_event_cursor" {
            return try JSONSerialization.data(withJSONObject: [
                "type": "event_cursor",
                "cursor": 0,
            ])
        }
        guard type == "wait_events" else {
            throw CocoaError(.coderReadCorrupt)
        }

        listRequestsByOwner[ownerUserID, default: 0] += 1
        activeListRequests += 1
        maximumConcurrentListRequestCount = max(
            maximumConcurrentListRequestCount,
            activeListRequests
        )
        defer { activeListRequests -= 1 }
        do {
            try await Task.sleep(for: listDelay)
        } catch is CancellationError {
            cancelledWaitRequests += 1
            throw CancellationError()
        }

        let shouldReturnEvent = eventOwners.contains(ownerUserID)
            && deliveredEventOwners.insert(ownerUserID).inserted
        let events: [[String: Any]] = shouldReturnEvent ? [[
            "cursor": 1,
            "event_id": "event-\(ownerUserID)",
            "run_id": "run-\(ownerUserID)",
            "event_type": "run_updated",
            "payload": ["conversation_id": "conversation-\(ownerUserID)"],
            "created_at_unix_ms": 1,
        ]] : []
        return try JSONSerialization.data(withJSONObject: [
            "type": "events",
            "events": events,
            "next_cursor": shouldReturnEvent ? 1 : 0,
        ])
    }

    func listEventRequestCount(ownerUserID: String? = nil) -> Int {
        if let ownerUserID {
            return listRequestsByOwner[ownerUserID, default: 0]
        }
        return listRequestsByOwner.values.reduce(0, +)
    }

    func activeListRequestCount() -> Int {
        activeListRequests
    }

    func maximumConcurrentListRequests() -> Int {
        maximumConcurrentListRequestCount
    }

    func cancelledWaitRequestCount() -> Int {
        cancelledWaitRequests
    }
}
