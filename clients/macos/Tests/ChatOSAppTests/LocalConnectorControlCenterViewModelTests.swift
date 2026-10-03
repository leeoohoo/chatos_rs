import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class LocalConnectorControlCenterViewModelTests: XCTestCase {
    func testLateUnpairedResultCannotOverwriteNewerPairedResult() async throws {
        let service = BrowserExtensionPairingServiceStub()
        let viewModel = LocalConnectorControlCenterViewModel(service: service)

        viewModel.refreshBrowserExtensionPairingStatus(pluginID: "browser")
        try await waitUntil { await service.hasDelayedRequest() }
        viewModel.refreshBrowserExtensionPairingStatus(pluginID: "browser")
        try await waitUntil {
            viewModel.browserExtensionPairedPluginIDs.contains("browser")
        }
        await service.resumeDelayedRequest()
        try await Task.sleep(for: .milliseconds(20))

        XCTAssertTrue(viewModel.browserExtensionPairedPluginIDs.contains("browser"))
    }

    func testTransientPairingFailurePreservesLastTrustedState() async throws {
        let service = BrowserExtensionPairingServiceStub()
        await service.skipDelay()
        let viewModel = LocalConnectorControlCenterViewModel(service: service)

        viewModel.refreshBrowserExtensionPairingStatus(pluginID: "browser")
        try await waitUntil {
            viewModel.browserExtensionPairedPluginIDs.contains("browser")
        }
        await service.failNextRequest()
        viewModel.refreshBrowserExtensionPairingStatus(pluginID: "browser")
        try await waitUntil { await service.requestCount() == 2 }
        try await Task.sleep(for: .milliseconds(20))

        XCTAssertTrue(viewModel.browserExtensionPairedPluginIDs.contains("browser"))
    }

    func testLateTabLoadCannotOverwriteNewerCommandHistory() async throws {
        let service = BrowserExtensionPairingServiceStub()
        let viewModel = LocalConnectorControlCenterViewModel(service: service)

        viewModel.loadCommandHistory()
        try await waitUntil { await service.hasDelayedHistoryRequest() }
        viewModel.loadCommandHistory()
        try await waitUntil { viewModel.commandHistory.first?.id == "new" }
        await service.resumeDelayedHistoryRequest()
        try await Task.sleep(for: .milliseconds(20))

        XCTAssertEqual(viewModel.commandHistory.map(\.id), ["new"])
        XCTAssertFalse(viewModel.isLoading)
    }

    func testSignedOutResetRejectsLateControlActionResult() async throws {
        let service = BrowserExtensionPairingServiceStub()
        let viewModel = LocalConnectorControlCenterViewModel(service: service)

        viewModel.disconnect()
        try await waitUntil { await service.hasDelayedDisconnect() }
        viewModel.resetForSignedOut()
        await service.resumeDelayedDisconnect()
        try await Task.sleep(for: .milliseconds(20))

        XCTAssertNil(viewModel.status)
        XCTAssertFalse(viewModel.isPerformingAction)
        XCTAssertNil(viewModel.notice)
        XCTAssertNil(viewModel.errorMessage)
    }

    func testSignedOutResetRejectsLateApprovalConsistencyResult() async throws {
        let service = BrowserExtensionPairingServiceStub()
        let viewModel = LocalConnectorControlCenterViewModel(service: service)

        viewModel.startApprovalMonitoring()
        try await waitUntil { await service.hasDelayedApprovalRequest() }
        viewModel.resetForSignedOut()
        await service.resumeDelayedApprovalRequest()
        try await Task.sleep(for: .milliseconds(20))

        XCTAssertTrue(viewModel.pendingApprovals.isEmpty)
    }

    func testSignedOutResetRejectsLateTaskModelCatalog() async throws {
        let service = BrowserExtensionPairingServiceStub()
        let viewModel = LocalConnectorControlCenterViewModel(service: service)

        let modelsTask = Task { try await viewModel.availableTaskModels() }
        try await waitUntil { await service.hasDelayedModelCatalogRequest() }
        viewModel.resetForSignedOut()
        await service.resumeDelayedModelCatalogRequest()

        do {
            _ = try await modelsTask.value
            XCTFail("Expected the previous account's model catalog to be rejected")
        } catch is CancellationError {
            // The catalog belongs to the signed-out lifecycle.
        }
        XCTAssertNil(viewModel.modelCatalog)
    }

    func testActivationWaitsForPendingSignedOutSuspension() async throws {
        let service = BrowserExtensionPairingServiceStub()
        await service.delayNextSuspension()
        let viewModel = LocalConnectorControlCenterViewModel(service: service)

        viewModel.resetForSignedOut()
        try await waitUntil { await service.hasDelayedSuspension() }
        viewModel.activate(pairIfNeeded: true)
        try await Task.sleep(for: .milliseconds(20))
        let pairCallsBeforeSuspension = await service.pairCallCount()
        XCTAssertEqual(pairCallsBeforeSuspension, 0)

        await service.resumeDelayedSuspension()
        try await waitUntil { await service.pairCallCount() == 1 }
        try await waitUntil { viewModel.status?.configured == true }
        let connected = await service.isConnected()
        XCTAssertTrue(connected)
    }

    func testSignedOutSuspensionWaitsForCancelledPairingToSettle() async throws {
        let service = BrowserExtensionPairingServiceStub()
        await service.delayNextPairing()
        let viewModel = LocalConnectorControlCenterViewModel(service: service)

        viewModel.activate(pairIfNeeded: true)
        try await waitUntil { await service.hasDelayedPairing() }
        viewModel.resetForSignedOut()
        try await Task.sleep(for: .milliseconds(20))
        let suspensionCallsBeforePairing = await service.suspensionCallCount()
        XCTAssertEqual(suspensionCallsBeforePairing, 0)

        await service.resumeDelayedPairing()
        try await waitUntil { await service.suspensionCallCount() == 1 }
        let connected = await service.isConnected()
        XCTAssertFalse(connected)
        XCTAssertNil(viewModel.status)
    }

    func testControlActionWaitsForPendingSignedOutSuspension() async throws {
        let service = BrowserExtensionPairingServiceStub()
        await service.delayNextSuspension()
        let viewModel = LocalConnectorControlCenterViewModel(service: service)

        viewModel.resetForSignedOut()
        try await waitUntil { await service.hasDelayedSuspension() }
        viewModel.disconnect()
        try await Task.sleep(for: .milliseconds(20))
        let disconnectStartedEarly = await service.hasDelayedDisconnect()
        XCTAssertFalse(disconnectStartedEarly)

        await service.resumeDelayedSuspension()
        try await waitUntil { await service.hasDelayedDisconnect() }
        await service.resumeDelayedDisconnect()
        try await waitUntil { viewModel.isPerformingAction == false }
    }

    func testActivationWaitsForConnectorServicePreparation() async throws {
        let service = BrowserExtensionPairingServiceStub()
        let gate = ConnectorServicePreparationGate()
        let viewModel = LocalConnectorControlCenterViewModel(service: service)
        let preparationTask = Task { await gate.wait() }
        viewModel.setServicePreparationTask(preparationTask)

        viewModel.activate(pairIfNeeded: true)
        try await Task.sleep(for: .milliseconds(20))
        let pairCallsBeforePreparation = await service.pairCallCount()
        XCTAssertEqual(pairCallsBeforePreparation, 0)

        await gate.open()
        try await waitUntil { await service.pairCallCount() == 1 }
        try await waitUntil { viewModel.status?.configured == true }
    }

    func testManualPluginRefreshBypassesShortLivedCatalogSnapshot() async throws {
        let service = BrowserExtensionPairingServiceStub()
        let viewModel = LocalConnectorControlCenterViewModel(service: service)

        viewModel.loadPlugins(forceRefresh: true)
        try await waitUntil { await service.pluginRefreshFlags().count == 1 }

        let refreshFlags = await service.pluginRefreshFlags()
        XCTAssertEqual(refreshFlags, [true])
        XCTAssertFalse(viewModel.isLoading)
    }

    func testManualPermissionRefreshBypassesPermissionSnapshot() async throws {
        let service = BrowserExtensionPairingServiceStub()
        let viewModel = LocalConnectorControlCenterViewModel(service: service)

        viewModel.refreshPluginPermissions(id: "computer-use")
        try await waitUntil { await service.pluginRefreshFlags().count == 1 }

        let refreshFlags = await service.pluginRefreshFlags()
        XCTAssertEqual(refreshFlags, [true])
        XCTAssertFalse(viewModel.isPerformingAction)
    }

    private func waitUntil(
        _ condition: @escaping @MainActor () async -> Bool
    ) async throws {
        for _ in 0..<100 {
            if await condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("Timed out waiting for connector control-center state")
    }
}

private actor ConnectorServicePreparationGate {
    private var continuation: CheckedContinuation<Void, Never>?
    private var isOpen = false

    func wait() async {
        guard !isOpen else { return }
        await withCheckedContinuation { continuation in
            self.continuation = continuation
        }
    }

    func open() {
        isOpen = true
        continuation?.resume()
        continuation = nil
    }
}

private actor BrowserExtensionPairingServiceStub: LocalConnectorControlServicing {
    private var pairingRequestCount = 0
    private var delayedRequestContinuation: CheckedContinuation<Void, Never>?
    private var shouldDelayFirstRequest = true
    private var shouldFailNextRequest = false
    private var commandHistoryRequestCount = 0
    private var delayedHistoryContinuation: CheckedContinuation<Void, Never>?
    private var delayedDisconnectContinuation: CheckedContinuation<Void, Never>?
    private var delayedApprovalContinuation: CheckedContinuation<Void, Never>?
    private var delayedModelCatalogContinuation: CheckedContinuation<Void, Never>?
    private var delayedPairingContinuation: CheckedContinuation<Void, Never>?
    private var delayedSuspensionContinuation: CheckedContinuation<Void, Never>?
    private var shouldDelayNextPairing = false
    private var shouldDelayNextSuspension = false
    private var pairingCalls = 0
    private var suspensionCalls = 0
    private var requestedPluginRefreshFlags: [Bool] = []
    private var connected = false

    func isBrowserExtensionPaired(pluginID: String) async throws -> Bool {
        pairingRequestCount += 1
        if pairingRequestCount == 1, shouldDelayFirstRequest {
            await withCheckedContinuation { continuation in
                delayedRequestContinuation = continuation
            }
            return false
        }
        if shouldFailNextRequest {
            shouldFailNextRequest = false
            throw URLError(.cannotConnectToHost)
        }
        return true
    }

    func hasDelayedRequest() -> Bool { delayedRequestContinuation != nil }
    func requestCount() -> Int { pairingRequestCount }

    func skipDelay() {
        shouldDelayFirstRequest = false
    }

    func failNextRequest() {
        shouldFailNextRequest = true
    }

    func resumeDelayedRequest() {
        delayedRequestContinuation?.resume()
        delayedRequestContinuation = nil
    }

    func hasDelayedHistoryRequest() -> Bool { delayedHistoryContinuation != nil }

    func resumeDelayedHistoryRequest() {
        delayedHistoryContinuation?.resume()
        delayedHistoryContinuation = nil
    }

    func hasDelayedDisconnect() -> Bool { delayedDisconnectContinuation != nil }

    func resumeDelayedDisconnect() {
        delayedDisconnectContinuation?.resume()
        delayedDisconnectContinuation = nil
    }

    func hasDelayedApprovalRequest() -> Bool { delayedApprovalContinuation != nil }

    func resumeDelayedApprovalRequest() {
        delayedApprovalContinuation?.resume()
        delayedApprovalContinuation = nil
    }

    func hasDelayedModelCatalogRequest() -> Bool { delayedModelCatalogContinuation != nil }

    func resumeDelayedModelCatalogRequest() {
        delayedModelCatalogContinuation?.resume()
        delayedModelCatalogContinuation = nil
    }

    func delayNextPairing() { shouldDelayNextPairing = true }
    func delayNextSuspension() { shouldDelayNextSuspension = true }
    func hasDelayedPairing() -> Bool { delayedPairingContinuation != nil }
    func hasDelayedSuspension() -> Bool { delayedSuspensionContinuation != nil }
    func pairCallCount() -> Int { pairingCalls }
    func suspensionCallCount() -> Int { suspensionCalls }
    func isConnected() -> Bool { connected }

    func resumeDelayedPairing() {
        delayedPairingContinuation?.resume()
        delayedPairingContinuation = nil
    }

    func resumeDelayedSuspension() {
        delayedSuspensionContinuation?.resume()
        delayedSuspensionContinuation = nil
    }

    func fetchStatus() async throws -> LocalConnectorStatus { throw StubError.unimplemented }
    func pairWithCurrentChatOSSession(deviceName: String?) async throws -> LocalConnectorStatus {
        pairingCalls += 1
        if shouldDelayNextPairing {
            shouldDelayNextPairing = false
            await withCheckedContinuation { continuation in
                delayedPairingContinuation = continuation
            }
        }
        connected = true
        return Self.status()
    }
    func suspendForSignedOut() async {
        suspensionCalls += 1
        if shouldDelayNextSuspension {
            shouldDelayNextSuspension = false
            await withCheckedContinuation { continuation in
                delayedSuspensionContinuation = continuation
            }
        }
        connected = false
    }
    func resumeServerAccess() async throws -> LocalConnectorStatus { throw StubError.unimplemented }
    func disconnect() async throws -> LocalConnectorStatus {
        await withCheckedContinuation { continuation in
            delayedDisconnectContinuation = continuation
        }
        return Self.status()
    }
    func fetchRuntimeSettings() async throws -> LocalConnectorRuntimeSettings { throw StubError.unimplemented }
    func updateDeveloperMode(_ enabled: Bool) async throws -> LocalConnectorRuntimeSettings { throw StubError.unimplemented }
    func fetchSystemPermissions() async throws -> LocalConnectorSystemPermissions { throw StubError.unimplemented }
    func requestSystemPermission(id: String) async throws -> LocalConnectorSystemPermissions { throw StubError.unimplemented }
    func executeTerminal(workspaceID: String, commandLine: String, cwd: String?) async throws -> LocalConnectorTerminalResult { throw StubError.unimplemented }
    func fetchCommandHistory(limit: Int) async throws -> [LocalConnectorCommandHistoryEntry] {
        commandHistoryRequestCount += 1
        if commandHistoryRequestCount == 1 {
            await withCheckedContinuation { continuation in
                delayedHistoryContinuation = continuation
            }
            return [Self.historyEntry(id: "old")]
        }
        return [Self.historyEntry(id: "new")]
    }
    func clearCommandHistory() async throws { throw StubError.unimplemented }
    func fetchApprovalSettings() async throws -> LocalConnectorApprovalSettings { throw StubError.unimplemented }
    func updateDefaultApprovalMode(_ mode: LocalConnectorApprovalMode, riskAcknowledged: Bool) async throws -> LocalConnectorApprovalSettings { throw StubError.unimplemented }
    func fetchPendingApprovals() async throws -> [LocalConnectorPendingApproval] {
        await withCheckedContinuation { continuation in
            delayedApprovalContinuation = continuation
        }
        return [Self.pendingApproval()]
    }
    func resolveApproval(id: String, decision: String) async throws { throw StubError.unimplemented }
    func fetchModelCatalog(refresh: Bool) async throws -> LocalConnectorModelCatalog {
        await withCheckedContinuation { continuation in
            delayedModelCatalogContinuation = continuation
        }
        return .init(
            items: [],
            settings: .init(
                modelRequestMaxRetries: nil,
                commandApprovalModelConfigID: nil,
                commandApprovalThinkingLevel: nil
            )
        )
    }
    func fetchSandboxBackends() async throws -> [LocalConnectorSandboxBackend] { throw StubError.unimplemented }
    func fetchSandboxSettings() async throws -> LocalConnectorSandboxSettings { throw StubError.unimplemented }
    func updateSandboxSettings(enabled: Bool?, permissionProfileID: String?, approvalPolicy: String?, approvalReviewer: String?, networkAccess: String?) async throws -> LocalConnectorSandboxSettings { throw StubError.unimplemented }
    func fetchPlugins() async throws -> [LocalConnectorPlugin] {
        requestedPluginRefreshFlags.append(false)
        return []
    }
    func fetchPlugins(refresh: Bool) async throws -> [LocalConnectorPlugin] {
        requestedPluginRefreshFlags.append(refresh)
        return []
    }
    func installPlugin(id: String) async throws { throw StubError.unimplemented }
    func uninstallPlugin(id: String) async throws { throw StubError.unimplemented }
    func updatePluginEnabled(id: String, enabled: Bool) async throws { throw StubError.unimplemented }

    func pluginRefreshFlags() -> [Bool] { requestedPluginRefreshFlags }

    private static func historyEntry(id: String) -> LocalConnectorCommandHistoryEntry {
        .init(
            id: id,
            source: "ui",
            workspaceAlias: nil,
            cwd: nil,
            display: id,
            status: "completed",
            exitCode: 0,
            stdoutPreview: nil,
            stderrPreview: nil,
            error: nil,
            startedAt: "2026-10-03T00:00:00Z"
        )
    }

    private static func status() -> LocalConnectorStatus {
        .init(
            configured: true,
            connectorRunning: false,
            developerMode: false,
            cloudBaseURL: nil,
            userServiceBaseURL: nil,
            deviceID: "device",
            deviceName: "Test Mac",
            user: nil,
            defaultWorkspaceID: nil,
            workspaces: []
        )
    }

    private static func pendingApproval() -> LocalConnectorPendingApproval {
        .init(
            id: "approval",
            requestID: "request",
            command: "pwd",
            cwd: "/tmp",
            source: "test",
            risk: "low",
            reason: nil,
            createdAt: "2026-10-03T00:00:00Z",
            availableDecisions: ["accept", "decline"]
        )
    }
}

private enum StubError: Error {
    case unimplemented
}
