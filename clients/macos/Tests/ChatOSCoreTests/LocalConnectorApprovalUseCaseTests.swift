import ChatOSCore
import Testing

struct LocalConnectorApprovalUseCaseTests {
    @Test
    func loadSnapshotFetchesSettingsAndPendingApprovals() async throws {
        let service = LocalConnectorApprovalServiceStub()
        let useCase = LocalConnectorApprovalUseCase(service: service)

        let snapshot = try await useCase.loadSnapshot()

        #expect(snapshot.settings.defaultMode == .requestApproval)
        #expect(snapshot.pendingApprovals.map(\.id) == ["approval-1"])
        #expect(await service.settingsFetchCount == 1)
        #expect(await service.pendingFetchCount == 1)
    }

    @Test
    func resolveMakesDecisionBeforeRefreshingDurableTruth() async throws {
        let service = LocalConnectorApprovalServiceStub()
        let useCase = LocalConnectorApprovalUseCase(service: service)

        let snapshot = try await useCase.resolve(
            approval: LocalConnectorApprovalServiceStub.approval,
            decision: .acceptForSession
        )

        #expect(await service.operations.first == "resolve:approval-1:acceptForSession")
        #expect(Set(await service.operations.dropFirst()) == ["settings", "pending"])
        #expect(snapshot.pendingApprovals.isEmpty)
    }

    @Test
    func resolveRejectsDecisionNotOfferedByApproval() async throws {
        let service = LocalConnectorApprovalServiceStub()
        let useCase = LocalConnectorApprovalUseCase(service: service)
        var approval = LocalConnectorApprovalServiceStub.approval
        approval.availableDecisions = [LocalConnectorApprovalDecision.decline.rawValue]

        await #expect(throws: LocalConnectorApprovalUseCaseError.decisionUnavailable) {
            _ = try await useCase.resolve(approval: approval, decision: .accept)
        }
        #expect(await service.operations.isEmpty)
    }

    @Test
    func updateModeForwardsRiskAcknowledgement() async throws {
        let service = LocalConnectorApprovalServiceStub()
        let useCase = LocalConnectorApprovalUseCase(service: service)

        let settings = try await useCase.updateDefaultMode(
            .fullControl,
            riskAcknowledged: true
        )

        #expect(settings.defaultMode == .fullControl)
        #expect(await service.modeUpdates == [
            ApprovalModeUpdate(mode: .fullControl, riskAcknowledged: true),
        ])
    }

    @Test
    func monitoringIntervalsPreserveStreamingAndFallbackBehavior() {
        #expect(LocalConnectorApprovalMonitoringPolicy.consistencyCheckInterval(
            hasStreamingService: true
        ) == .seconds(60))
        #expect(LocalConnectorApprovalMonitoringPolicy.consistencyCheckInterval(
            hasStreamingService: false
        ) == .seconds(2))
    }
}

private struct ApprovalModeUpdate: Sendable, Equatable {
    var mode: LocalConnectorApprovalMode
    var riskAcknowledged: Bool
}

private actor LocalConnectorApprovalServiceStub: LocalConnectorApprovalServicing {
    private(set) var settingsFetchCount = 0
    private(set) var pendingFetchCount = 0
    private(set) var modeUpdates: [ApprovalModeUpdate] = []
    private(set) var operations: [String] = []
    private var resolved = false

    func fetchApprovalSettings() async throws -> LocalConnectorApprovalSettings {
        settingsFetchCount += 1
        operations.append("settings")
        return LocalConnectorApprovalSettings(
            defaultMode: modeUpdates.last?.mode ?? .requestApproval,
            history: []
        )
    }

    func updateDefaultApprovalMode(
        _ mode: LocalConnectorApprovalMode,
        riskAcknowledged: Bool
    ) async throws -> LocalConnectorApprovalSettings {
        modeUpdates.append(ApprovalModeUpdate(
            mode: mode,
            riskAcknowledged: riskAcknowledged
        ))
        return LocalConnectorApprovalSettings(defaultMode: mode, history: [])
    }

    func fetchPendingApprovals() async throws -> [LocalConnectorPendingApproval] {
        pendingFetchCount += 1
        operations.append("pending")
        return resolved ? [] : [Self.approval]
    }

    func resolveApproval(
        id: String,
        decision: LocalConnectorApprovalDecision
    ) async throws {
        operations.append("resolve:\(id):\(decision.rawValue)")
        resolved = true
    }

    static let approval = LocalConnectorPendingApproval(
        id: "approval-1",
        requestID: "request-1",
        command: "git status",
        cwd: "/workspace",
        source: "task",
        risk: "low",
        reason: nil,
        createdAt: "2026-10-19T00:00:00Z",
        availableDecisions: ["decline", "accept", "acceptForSession"]
    )
}
