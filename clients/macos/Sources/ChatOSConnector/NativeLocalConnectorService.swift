import ChatOSAgentRuntime
import ChatOSCore
import Foundation
import OSLog

public typealias NativeApprovalMemoryProviderFactory = @Sendable (
    _ tenantID: String,
    _ workspaceID: String,
    _ runID: UUID,
    _ runtimeScope: String
) async throws -> AgentMemoryContextProvider

public struct NativeConnectorConfiguration: Sendable {
    public var gatewayBaseURL: URL
    public var stateURL: URL
    public var deploymentIdentifier: String

    public init(
        gatewayBaseURL: URL,
        stateURL: URL,
        deploymentIdentifier: String = "default"
    ) {
        self.gatewayBaseURL = gatewayBaseURL
        self.stateURL = stateURL
        self.deploymentIdentifier = deploymentIdentifier
    }
}

public actor NativeLocalConnectorService: LocalConnectorControlServicing, LocalConnectorApprovalStreaming {
    static let accessTokenAccount = "gateway-access-token-v1"
    static let logger = Logger(
        subsystem: "com.chatos.swift-client",
        category: "NativeLocalConnector"
    )

    let configuration: NativeConnectorConfiguration
    let ticketProvider: any LocalConnectorPairingTicketProviding
    let gateway: NativeConnectorGateway
    let stateStore: NativeConnectorStateStore
    let pluginInstaller: NativePluginInstaller
    let mcpCodeWriteStore = NativeMCPCodeWriteStore()
    let mcpTerminalStore = NativeMCPTerminalStore()
    let pluginRuntimeStore = NativePluginRuntimeStore()
    var pluginSkillRuntimeSessions: [String: NativePluginSkillRuntimeSession] = [:]
    let pluginApplicationRuntime = NativePluginApplicationRuntime()
    let browserExtensionPairingRuntime = NativeBrowserExtensionPairingRuntime()
    let pluginRuntimeRootURL: URL
    let remoteConnectionRuntime: (any NativeRemoteConnectionRuntimeProviding)?
    let approvalMemoryProviderFactory: NativeApprovalMemoryProviderFactory?
    weak var companionRuntime: (any LocalConnectorCompanionRuntimeProviding)?
    let secretStore: NativeConnectorSecretStore
    var state: NativeConnectorPersistentState
    var cachedAccessToken: String?
    var hasLoadedAccessToken = false
    var cachedDeviceIdentity: NativeConnectorDeviceIdentity?
    var managedRuntimeConfigCache: NativeManagedRuntimeConfigCache?
    var managedRuntimeConfigRefresh: NativeManagedRuntimeConfigRefresh?
    var managedRuntimeConfigGeneration = 0
    var gatewayConnected = false
    var webSocket: URLSessionWebSocketTask?
    var receiveTask: Task<Void, Never>?
    var heartbeatTask: Task<Void, Never>?
    var reconnectTask: Task<Void, Never>?
    var shouldMaintainGatewayConnection = false
    var isSystemSleeping = false
    var lastGatewayPongAt: Date?
    var gatewayReconnectFailureCount = 0
    var gatewayConnectionCleanupCount = 0
    var lastConnectorCredentialRefreshAttemptAt: Date?
    var pendingApprovals: [LocalConnectorPendingApproval] = []
    var pendingApprovalContinuations: [String: CheckedContinuation<NativeApprovalDecision, Never>] = [:]
    var pendingApprovalScopeKeys: [String: String] = [:]
    var approvalSnapshotContinuations: [
        UUID: AsyncStream<[LocalConnectorPendingApproval]>.Continuation
    ] = [:]
    var approvalEventContinuations: [
        UUID: AsyncStream<LocalConnectorApprovalEvent>.Continuation
    ] = [:]
    var sessionApprovalAllowlist: Set<String> = []
    var seenRelayNonces: [String: Int64] = [:]
    var terminalRelaySessions: [String: any NativeTerminalRelaySessionProtocol] = [:]
    let terminalRelayEventPump = NativeTerminalRelayEventPump()
    var terminalRelayEventTask: Task<Void, Never>?

    public init(
        configuration: NativeConnectorConfiguration,
        ticketProvider: any LocalConnectorPairingTicketProviding,
        remoteConnectionRuntime: (any NativeRemoteConnectionRuntimeProviding)? = nil,
        approvalMemoryProviderFactory: NativeApprovalMemoryProviderFactory? = nil
    ) {
        self.configuration = configuration
        self.ticketProvider = ticketProvider
        self.gateway = NativeConnectorGateway(baseURL: configuration.gatewayBaseURL)
        self.stateStore = NativeConnectorStateStore(stateURL: configuration.stateURL)
        self.secretStore = NativeConnectorSecretStore(
            rootURL: configuration.stateURL.deletingLastPathComponent()
                .appendingPathComponent("Secrets", isDirectory: true)
        )
        self.pluginInstaller = NativePluginInstaller(
            rootURL: configuration.stateURL
                .deletingLastPathComponent()
                .appendingPathComponent("Plugins", isDirectory: true)
        )
        self.pluginRuntimeRootURL = configuration.stateURL
            .deletingLastPathComponent()
            .appendingPathComponent("PluginRuntime", isDirectory: true)
        self.remoteConnectionRuntime = remoteConnectionRuntime
        self.approvalMemoryProviderFactory = approvalMemoryProviderFactory
        self.state = (try? stateStore.load()) ?? .empty
    }

    public func fetchStatus() async throws -> LocalConnectorStatus {
        if pairingMatchesCurrentDeployment,
           state.deviceID != nil,
           state.gatewayConnectionEnabled != false,
           !gatewayConnected {
            try? await connectGateway()
        }
        return statusSnapshot()
    }

    public func setCompanionRuntime(
        _ runtime: (any LocalConnectorCompanionRuntimeProviding)?
    ) {
        companionRuntime = runtime
    }

    public func pairWithCurrentChatOSSession(deviceName: String?) async throws -> LocalConnectorStatus {
        invalidateManagedRuntimeConfig()
        let resolvedName = deviceName?.trimmedNonEmpty ?? Host.current().localizedName ?? "Mac"
        let ticket = try await ticketProvider.issueLocalConnectorPairingTicket()
        let login = try await gateway.exchange(ticket: ticket, deviceName: resolvedName)
        try secretStore.save(Data(login.token.utf8), account: Self.accessTokenAccount)
        cachedAccessToken = login.token
        hasLoadedAccessToken = true

        if canReuseExistingPairing(ownerUserID: login.user.id),
           let deviceID = state.deviceID {
            do {
                let existingDevice = try await gateway.device(token: login.token, id: deviceID)
                guard existingDevice.ownerUserID == login.user.id else {
                    throw NativeConnectorError.server(
                        status: 409,
                        message: "当前登录账号与已配对设备账号不一致"
                    )
                }
                let identity = try deviceIdentity()
                let workspace = try await ensureDefaultWorkspace(
                    token: login.token,
                    deviceID: deviceID,
                    publicKey: identity.publicKey
                )
                state.user = login.user.domainModel
                state.deviceName = existingDevice.displayName
                state.workspaces = [workspace]
                state.gatewayConnectionEnabled = true
                lastConnectorCredentialRefreshAttemptAt = nil
                try stateStore.save(state)
                try await connectGateway()
                try? await Task.sleep(for: .milliseconds(200))
                return statusSnapshot()
            } catch let error as NativeConnectorError where Self.deviceMustBeRecreated(after: error) {
                // The local pairing belongs to this account/deployment, but the server-side
                // device was removed. Fall through to create a replacement exactly once.
            }
        }

        let identity = try deviceIdentity()
        let device = try await gateway.createDevice(
            token: login.token,
            displayName: resolvedName,
            publicKey: identity.publicKey
        )
        let workspace = try await ensureDefaultWorkspace(
            token: login.token,
            deviceID: device.id,
            publicKey: identity.publicKey
        )
        state.user = login.user.domainModel
        state.deploymentIdentifier = configuration.deploymentIdentifier
        state.gatewayBaseURL = normalizedGatewayBaseURL
        state.deviceID = device.id
        state.deviceName = resolvedName
        state.workspaces = [workspace]
        state.gatewayConnectionEnabled = true
        lastConnectorCredentialRefreshAttemptAt = nil
        try stateStore.save(state)
        try await connectGateway()
        try? await Task.sleep(for: .milliseconds(200))
        return statusSnapshot()
    }

    func canReuseExistingPairing(ownerUserID: String) -> Bool {
        pairingMatchesCurrentDeployment
            && state.deviceID?.trimmedNonEmpty != nil
            && state.user?.id == ownerUserID
    }

    static func deviceMustBeRecreated(after error: NativeConnectorError) -> Bool {
        switch error {
        case let .server(status, _): status == 403 || status == 404
        default: false
        }
    }

    public func suspendForSignedOut() async {
        await stopServerAccess()
        await pluginApplicationRuntime.stopAll()
        await browserExtensionPairingRuntime.stop()
    }

    public func resumeServerAccess() async throws -> LocalConnectorStatus {
        guard state.deviceID != nil else { throw NativeConnectorError.notPaired }
        state.gatewayConnectionEnabled = true
        try stateStore.save(state)
        try await connectGateway()
        return statusSnapshot()
    }

    public func disconnect() async throws -> LocalConnectorStatus {
        let token = try? accessToken()
        state.gatewayConnectionEnabled = false
        await stopServerAccess()
        try stateStore.save(state)
        if let deviceID = state.deviceID, let token {
            try? await gateway.disconnectDevice(token: token, id: deviceID)
        }
        return statusSnapshot()
    }

    private func stopServerAccess() async {
        invalidateManagedRuntimeConfig()
        shouldMaintainGatewayConnection = false
        gatewayReconnectFailureCount = 0
        closeAllTerminalRelaySessions()
        await stopGatewayConnection()
    }

    public func prepareForSystemSleep() async {
        guard state.deviceID != nil else { return }
        isSystemSleeping = true
        reconnectTask?.cancel()
        reconnectTask = nil
        await closeGatewayConnection(terminatePluginSessions: true)
        await pluginApplicationRuntime.stopAll()
        await browserExtensionPairingRuntime.stop()
    }

    public func recoverGatewayConnection(forceReconnect: Bool = false) async {
        guard state.deviceID != nil, state.gatewayConnectionEnabled != false else { return }
        isSystemSleeping = false
        shouldMaintainGatewayConnection = true
        if forceReconnect {
            gatewayReconnectFailureCount = 0
        }
        if forceReconnect, webSocket != nil {
            await closeGatewayConnection(terminatePluginSessions: true)
        }
        guard webSocket == nil else { return }
        scheduleGatewayReconnect()
    }

    public func fetchRuntimeSettings() async throws -> LocalConnectorRuntimeSettings {
        runtimeSettingsSnapshot()
    }

    public func updateDeveloperMode(_ enabled: Bool) async throws -> LocalConnectorRuntimeSettings {
        state.developerMode = enabled
        try stateStore.save(state)
        return runtimeSettingsSnapshot()
    }

    public func fetchSystemPermissions() async throws -> LocalConnectorSystemPermissions {
        NativeSystemPermissions.snapshot()
    }

    public func requestSystemPermission(id: String) async throws -> LocalConnectorSystemPermissions {
        await MainActor.run { NativeSystemPermissions.request(id) }
        return NativeSystemPermissions.snapshot()
    }

    public func executeTerminal(
        workspaceID: String,
        commandLine: String,
        cwd: String?
    ) async throws -> LocalConnectorTerminalResult {
        guard let workspace = state.workspaces.first(where: { $0.id == workspaceID }) else {
            throw NativeConnectorError.workspaceUnavailable
        }
        let result = try NativeTerminalExecutor.execute(
            command: "/bin/zsh",
            args: ["-lc", commandLine],
            cwd: cwd ?? workspace.absoluteRoot,
            workspace: workspace
        )
        let now = ISO8601DateFormatter().string(from: Date())
        state.commandHistory.insert(
            .init(
                id: UUID().uuidString,
                source: "native-terminal",
                workspaceAlias: workspace.alias,
                cwd: result.cwd,
                display: commandLine,
                status: result.success ? "completed" : "failed",
                exitCode: result.exitCode,
                stdoutPreview: result.stdout.prefixText(2_000),
                stderrPreview: result.stderr.prefixText(2_000),
                error: result.error,
                startedAt: now
            ),
            at: 0
        )
        state.commandHistory = Array(state.commandHistory.prefix(1_000))
        try stateStore.save(state)
        return result
    }

    public func fetchCommandHistory(limit: Int) async throws -> [LocalConnectorCommandHistoryEntry] {
        Array(state.commandHistory.prefix(max(1, min(limit, 200))))
    }

    public func clearCommandHistory() async throws {
        state.commandHistory = []
        try stateStore.save(state)
    }

    public func fetchApprovalSettings() async throws -> LocalConnectorApprovalSettings {
        .init(defaultMode: state.approvalMode, history: state.approvalHistory)
    }

    public func updateDefaultApprovalMode(
        _ mode: LocalConnectorApprovalMode,
        riskAcknowledged: Bool
    ) async throws -> LocalConnectorApprovalSettings {
        if mode != .requestApproval, !riskAcknowledged {
            throw NativeConnectorError.server(
                status: 409,
                message: "提高审批权限前需要明确确认风险。"
            )
        }
        if mode == .autoApproval,
           state.commandApprovalModelConfigID?.trimmedNonEmpty == nil {
            throw NativeConnectorError.server(
                status: 409,
                message: "请先在 AI 模型配置中选择本机审批 Agent 模型。"
            )
        }
        state.approvalMode = mode
        try stateStore.save(state)
        return .init(defaultMode: mode, history: state.approvalHistory)
    }

    public func fetchPendingApprovals() async throws -> [LocalConnectorPendingApproval] {
        pendingApprovals
    }

    public func approvalSnapshots() async -> AsyncStream<[LocalConnectorPendingApproval]> {
        AsyncStream { continuation in
            let id = UUID()
            approvalSnapshotContinuations[id] = continuation
            continuation.yield(pendingApprovals)
            continuation.onTermination = { [weak self] _ in
                Task { await self?.removeApprovalSnapshotContinuation(id) }
            }
        }
    }

    public func approvalEvents() async -> AsyncStream<LocalConnectorApprovalEvent> {
        AsyncStream { continuation in
            let id = UUID()
            approvalEventContinuations[id] = continuation
            continuation.onTermination = { [weak self] _ in
                Task { await self?.removeApprovalEventContinuation(id) }
            }
        }
    }

    public func resolveApproval(id: String, decision: String) async throws {
        let pending = pendingApprovals.first(where: { $0.id == id })
        pendingApprovals.removeAll(where: { $0.id == id })
        publishApprovalSnapshot()
        let continuation = pendingApprovalContinuations.removeValue(forKey: id)
        let approvalScopeKey = pendingApprovalScopeKeys.removeValue(forKey: id)
        if let pending {
            state.approvalHistory.insert(
                .init(
                    id: UUID().uuidString,
                    command: pending.command,
                    cwd: pending.cwd,
                    source: pending.source,
                    mode: state.approvalMode,
                    decision: decision,
                    risk: pending.risk,
                    reason: pending.reason,
                    createdAt: ISO8601DateFormatter().string(from: Date())
                ),
                at: 0
            )
            try stateStore.save(state)
            publishApprovalEvent(.init(
                requestID: pending.requestID,
                command: pending.command,
                cwd: pending.cwd,
                source: pending.source,
                risk: pending.risk,
                decision: ["accept", "acceptForSession", "approve"].contains(decision)
                    ? "approved"
                    : "denied",
                reason: decision == "acceptForSession"
                    ? "用户已允许当前会话继续执行此类操作。"
                    : (decision == "decline" ? "用户已拒绝这次操作。" : "用户已允许这次操作。"),
                mode: state.approvalMode,
                reviewer: .user
            ))
        }
        switch decision {
        case "accept", "acceptForSession", "approve":
            if decision == "acceptForSession", let approvalScopeKey {
                sessionApprovalAllowlist.insert(approvalScopeKey)
            }
            continuation?.resume(returning: .approve(
                reason: "用户已在本机批准。",
                rememberAllow: decision == "acceptForSession"
            ))
        default:
            continuation?.resume(returning: .deny(reason: "用户已在本机拒绝。"))
        }
    }

    public func fetchSandboxBackends() async throws -> [LocalConnectorSandboxBackend] {
        [
            .init(
                backend: "native-macos",
                status: "ready",
                selectable: true,
                filesystemIsolation: true,
                networkIsolation: true,
                processTreeControl: true,
                message: "由 Swift Native Connector 在本机执行权限边界。"
            ),
        ]
    }

    public func fetchSandboxSettings() async throws -> LocalConnectorSandboxSettings {
        sandboxSettingsSnapshot()
    }

    public func updateSandboxSettings(
        enabled: Bool?,
        permissionProfileID: String?,
        approvalPolicy: String?,
        approvalReviewer: String?,
        networkAccess: String?
    ) async throws -> LocalConnectorSandboxSettings {
        if let enabled { state.sandboxEnabled = enabled }
        if let permissionProfileID { state.permissionProfileID = permissionProfileID }
        if let approvalPolicy { state.approvalPolicy = approvalPolicy }
        if let approvalReviewer { state.approvalReviewer = approvalReviewer }
        if let networkAccess { state.networkAccess = networkAccess }
        state.policyRevision = "native-\(ISO8601DateFormatter().string(from: Date()))"
        try stateStore.save(state)
        return sandboxSettingsSnapshot()
    }

    private func statusSnapshot() -> LocalConnectorStatus {
        .init(
            configured: pairingMatchesCurrentDeployment
                && state.deviceID != nil
                && (try? accessToken()) != nil,
            connectorRunning: gatewayConnected,
            developerMode: state.developerMode,
            cloudBaseURL: configuration.gatewayBaseURL.absoluteString,
            userServiceBaseURL: configuration.gatewayBaseURL.absoluteString,
            deviceID: state.deviceID,
            deviceName: state.deviceName,
            user: state.user,
            defaultWorkspaceID: state.workspaces.first?.id,
            workspaces: state.workspaces
        )
    }

    var pairingMatchesCurrentDeployment: Bool {
        if configuration.deploymentIdentifier == "default",
           state.deploymentIdentifier == nil,
           state.gatewayBaseURL == nil {
            return true
        }
        return state.deploymentIdentifier == configuration.deploymentIdentifier
            && state.gatewayBaseURL == normalizedGatewayBaseURL
    }

    private var normalizedGatewayBaseURL: String {
        configuration.gatewayBaseURL.absoluteString
            .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    }

    private func runtimeSettingsSnapshot() -> LocalConnectorRuntimeSettings {
        .init(
            developerMode: state.developerMode,
            developerCloudBaseURL: configuration.gatewayBaseURL.absoluteString,
            developerUserServiceBaseURL: configuration.gatewayBaseURL.absoluteString,
            developerChatOSWebURL: ""
        )
    }

    private func sandboxSettingsSnapshot() -> LocalConnectorSandboxSettings {
        .init(
            enabled: state.sandboxEnabled,
            defaultBackend: "native-macos",
            defaultPermissionProfileID: state.permissionProfileID,
            defaultPermissionProfileName: state.permissionProfileID,
            defaultApprovalPolicy: state.approvalPolicy,
            defaultApprovalReviewer: state.approvalReviewer,
            defaultNetworkAccess: state.networkAccess,
            permissionConfigurationError: nil,
            policyRevision: state.policyRevision
        )
    }

    func publishApprovalSnapshot() {
        let snapshot = pendingApprovals
        for continuation in approvalSnapshotContinuations.values {
            continuation.yield(snapshot)
        }
    }

    func publishApprovalEvent(_ event: LocalConnectorApprovalEvent) {
        for continuation in approvalEventContinuations.values {
            continuation.yield(event)
        }
    }

    private func removeApprovalSnapshotContinuation(_ id: UUID) {
        approvalSnapshotContinuations.removeValue(forKey: id)
    }

    private func removeApprovalEventContinuation(_ id: UUID) {
        approvalEventContinuations.removeValue(forKey: id)
    }

    func accessToken() throws -> String? {
        if hasLoadedAccessToken { return cachedAccessToken }
        guard let data = try secretStore.load(account: Self.accessTokenAccount) else {
            cachedAccessToken = nil
            hasLoadedAccessToken = true
            return nil
        }
        cachedAccessToken = String(data: data, encoding: .utf8)?.trimmedNonEmpty
        hasLoadedAccessToken = true
        return cachedAccessToken
    }

    func requireAccessToken() throws -> String {
        guard let token = try accessToken() else { throw NativeConnectorError.notPaired }
        return token
    }

    func deviceIdentity() throws -> NativeConnectorDeviceIdentity {
        if let cachedDeviceIdentity { return cachedDeviceIdentity }
        let identity = try NativeConnectorDeviceIdentity(secretStore: secretStore)
        cachedDeviceIdentity = identity
        return identity
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }

    func prefixText(_ maximum: Int) -> String? {
        guard !isEmpty else { return nil }
        return String(prefix(maximum))
    }
}
