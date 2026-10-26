import ChatOSNetworking
import ChatOSWorkerRuntime
@testable import ChatOSConnector
import XCTest

final class NativeClientMachineResourceSnapshotTests: XCTestCase {
    func testHostProtocolSnapshotReplacesConfiguredOnlyEstimate() async throws {
        let agentGovernor = LocalAgentExecutionGovernor(limit: 2)
        let fileGovernor = ClientFileWorkerResourceGovernor(
            budget: .init(
                totalConcurrentWorkers: 3,
                maximumMediaWorkers: 2,
                maximumCodeReadWorkers: 2
            )
        )
        let snapshot = NativeClientMachineResourceSnapshotProvider.assemble(
            logicalJobBudget: .init(totalLogicalJobs: 5, localAgentHostWorkItems: 2),
            agentSnapshot: await agentGovernor.snapshot(),
            hostSnapshot: .init(
                maximumConcurrentWorkItems: 3,
                activeWorkItems: 2,
                idleWorkers: 1,
                queuedReadyWorkItems: 4
            ),
            fileSnapshot: await fileGovernor.snapshot(),
            transferSnapshot: await ClientLargeTransferGovernor(limit: 1).snapshot(),
            heavyMediaSnapshot: await ClientHeavyMediaWorkGovernor(limit: 1).snapshot()
        )

        let host = try XCTUnwrap(snapshot.domain(.localAgentHostWorkItem))
        XCTAssertEqual(host.observationLevel, .exact)
        XCTAssertEqual(host.configuredLimit, 3)
        XCTAssertEqual(host.activeCount, 2)
        XCTAssertEqual(host.queuedCount, 4)
        XCTAssertFalse(snapshot.domainsMissingExactObservation.contains(.localAgentHostWorkItem))
    }

    func testSnapshotFreezesEveryResourceOwnerWithoutHidingObservationGaps() async throws {
        let agentGovernor = LocalAgentExecutionGovernor(limit: 2)
        let firstAgentValue = await agentGovernor.acquire()
        let firstAgent = try XCTUnwrap(firstAgentValue)
        let secondAgentValue = await agentGovernor.acquire()
        let secondAgent = try XCTUnwrap(secondAgentValue)
        let queuedAgent = Task { await agentGovernor.acquire() }
        try await waitUntil { await agentGovernor.snapshot().queuedCount == 1 }

        let fileGovernor = ClientFileWorkerResourceGovernor(
            budget: .init(
                totalConcurrentWorkers: 3,
                maximumMediaWorkers: 2,
                maximumCodeReadWorkers: 2
            )
        )
        let firstMedia = try await fileGovernor.acquire(context: .init(
            kind: .media,
            priority: .interactive,
            operation: "first-media"
        ))
        let secondMedia = try await fileGovernor.acquire(context: .init(
            kind: .media,
            priority: .interactive,
            operation: "second-media"
        ))
        let queuedMedia = Task {
            try await fileGovernor.acquire(context: .init(
                kind: .media,
                priority: .background,
                operation: "queued-media"
            ))
        }
        try await waitUntil { await fileGovernor.snapshot().queued.count == 1 }

        let transferGovernor = ClientLargeTransferGovernor(limit: 1)
        let transfer = try await transferGovernor.acquire(context: .init(
            taskID: "transfer",
            direction: .download
        ))
        let heavyMediaGovernor = ClientHeavyMediaWorkGovernor(limit: 1)
        let heavyMedia = try await heavyMediaGovernor.acquire(context: .init(
            taskID: "decode",
            kind: .imageDecode,
            estimatedDecodedBytes: 1_024
        ))

        let snapshot = NativeClientMachineResourceSnapshotProvider.assemble(
            logicalJobBudget: .init(totalLogicalJobs: 5, localAgentHostWorkItems: 2),
            agentSnapshot: await agentGovernor.snapshot(),
            fileSnapshot: await fileGovernor.snapshot(),
            transferSnapshot: await transferGovernor.snapshot(),
            heavyMediaSnapshot: await heavyMediaGovernor.snapshot()
        )

        XCTAssertEqual(snapshot.domains.map(\.domain), NativeClientResourceDomain.allCases)
        let agent = try XCTUnwrap(snapshot.domain(.swiftAgentRun))
        XCTAssertEqual(agent.admissionBoundary, .beforeDurableClaim)
        XCTAssertEqual(agent.durableClaimStateWhileQueued, .unclaimed)
        XCTAssertEqual(agent.configuredLimit, 2)
        XCTAssertEqual(agent.sharedPoolLimit, 5)
        XCTAssertEqual(agent.activeCount, 2)
        XCTAssertEqual(agent.queuedCount, 1)
        XCTAssertTrue(agent.permitsUnifiedClaimAdmission)

        let host = try XCTUnwrap(snapshot.domain(.localAgentHostWorkItem))
        XCTAssertEqual(host.admissionBoundary, .dedicatedWorkerBeforeDurableClaim)
        XCTAssertEqual(host.observationLevel, .configuredLimitOnly)
        XCTAssertEqual(host.configuredLimit, 2)
        XCTAssertNil(host.activeCount)
        XCTAssertNil(host.queuedCount)

        let media = try XCTUnwrap(snapshot.domain(.mediaWorker))
        XCTAssertEqual(media.activeCount, 2)
        XCTAssertEqual(media.queuedCount, 1)
        XCTAssertEqual(media.sharedPoolLimit, 3)
        XCTAssertEqual(snapshot.domain(.codeReadWorker)?.activeCount, 0)

        XCTAssertEqual(snapshot.domain(.largeTransfer)?.activeCount, 1)
        XCTAssertEqual(snapshot.domain(.heavyInProcessMedia)?.activeCount, 1)
        XCTAssertEqual(
            snapshot.domainsMissingExactObservation,
            [.localAgentHostWorkItem, .pluginProcess, .databaseMaintenance]
        )
        XCTAssertEqual(
            snapshot.domainsUnsafeForOuterClaimPermit,
            [.largeTransfer, .heavyInProcessMedia, .pluginProcess]
        )

        queuedAgent.cancel()
        _ = await queuedAgent.value
        queuedMedia.cancel()
        _ = try? await queuedMedia.value
        await firstAgent.releaseAndWait()
        await secondAgent.releaseAndWait()
        await firstMedia.releaseAndWait()
        await secondMedia.releaseAndWait()
        await transfer.releaseAndWait()
        await heavyMedia.releaseAndWait()
    }

    private func waitUntil(
        _ condition: @escaping @Sendable () async -> Bool
    ) async throws {
        for _ in 0..<200 {
            if await condition() { return }
            try await Task.sleep(for: .milliseconds(5))
        }
        XCTFail("Timed out waiting for resource governor state")
    }
}
