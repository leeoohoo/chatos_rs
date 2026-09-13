import ChatOSCore
import CryptoKit
import Foundation
import OSLog

public struct NativeConnectorConfiguration: Sendable {
    public var gatewayBaseURL: URL
    public var supportRootURL: URL
    let initialPairingState: NativeConnectorPairingState

    public init(gatewayBaseURL: URL, supportRootURL: URL) {
        self.gatewayBaseURL = gatewayBaseURL
        self.supportRootURL = supportRootURL
        self.initialPairingState = .empty
    }

    init(
        gatewayBaseURL: URL,
        supportRootURL: URL,
        testingPairingState: NativeConnectorPairingState
    ) {
        self.gatewayBaseURL = gatewayBaseURL
        self.supportRootURL = supportRootURL
        self.initialPairingState = testingPairingState
    }
}

public actor NativeLocalConnectorService: LocalConnectorControlServicing, LocalConnectorApprovalStreaming {
    private static let logger = Logger(
        subsystem: "com.chatos.swift-client",
        category: "NativeLocalConnector"
    )

    private let configuration: NativeConnectorConfiguration
    private let ticketProvider: any LocalConnectorPairingTicketProviding
    let gateway: NativeConnectorGateway
    let routeStore: NativeConnectorRouteStore
    let pluginInstaller: NativePluginInstaller
    let mcpCodeWriteStore = NativeMCPCodeWriteStore()
    let mcpTerminalStore = NativeMCPTerminalStore()
    let pluginRuntimeStore = NativePluginRuntimeStore()
    var pluginSkillRuntimeSessions: [String: NativePluginSkillRuntimeSession] = [:]
    let pluginApplicationRuntime = NativePluginApplicationRuntime()
    let browserExtensionPairingRuntime = NativeBrowserExtensionPairingRuntime()
    let agentRuntimeSettings: any AgentRuntimePreferencesProviding
    let terminalHistoryStore: NativeTerminalHistoryStore
    let runtimePreferencesStore: NativeConnectorRuntimePreferencesStore
    let approvalStore: NativeConnectorApprovalStore
    let accountSession: any NativeLocalAgentAccountSessionAccess
    let pluginStateStore: NativePluginStateStore
    let pairingStateStore: NativeConnectorPairingStateStore
    let pluginRuntimeRootURL: URL
    let remoteConnectionRuntime: (any NativeRemoteConnectionRuntimeProviding)?
    private let secretStore = NativeConnectorSecretStore()
    var pairingState: NativeConnectorPairingState
    private var activeClientStorageOwnerID: String?
    private var cachedAccessToken: String?
    private var hasLoadedAccessToken = false
    private var cachedDeviceIdentity: NativeConnectorDeviceIdentity?
    var managedRuntimeConfigCache: NativeManagedRuntimeConfigCache?
    var managedRuntimeConfigRefresh: NativeManagedRuntimeConfigRefresh?
    var managedRuntimeConfigGeneration = 0
    private var gatewayConnected = false
    var webSocket: URLSessionWebSocketTask?
    private var receiveTask: Task<Void, Never>?
    private var heartbeatTask: Task<Void, Never>?
    private var reconnectTask: Task<Void, Never>?
    private var shouldMaintainGatewayConnection = false
    private var isSystemSleeping = false
    private var lastGatewayPongAt: Date?
    private var gatewayReconnectFailureCount = 0
    private var gatewayConnectionCleanupCount = 0
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

    public init(
        configuration: NativeConnectorConfiguration,
        ticketProvider: any LocalConnectorPairingTicketProviding,
        routeStore: NativeConnectorRouteStore = .init(),
        accountSession: any NativeLocalAgentAccountSessionAccess,
        agentRuntimeSettings: any AgentRuntimePreferencesProviding,
        remoteConnectionRuntime: (any NativeRemoteConnectionRuntimeProviding)? = nil
    ) {
        self.configuration = configuration
        self.ticketProvider = ticketProvider
        self.gateway = NativeConnectorGateway(baseURL: configuration.gatewayBaseURL)
        self.routeStore = routeStore
        self.terminalHistoryStore = NativeTerminalHistoryStore(accountSession: accountSession)
        self.runtimePreferencesStore = NativeConnectorRuntimePreferencesStore(
            accountSession: accountSession
        )
        self.approvalStore = NativeConnectorApprovalStore(accountSession: accountSession)
        self.accountSession = accountSession
        self.pluginStateStore = NativePluginStateStore(accountSession: accountSession)
        self.pairingStateStore = NativeConnectorPairingStateStore(accountSession: accountSession)
        self.agentRuntimeSettings = agentRuntimeSettings
        self.pluginInstaller = NativePluginInstaller(
            rootURL: configuration.supportRootURL
                .appendingPathComponent("Plugins", isDirectory: true)
        )
        self.pluginRuntimeRootURL = configuration.supportRootURL
            .appendingPathComponent("PluginRuntime", isDirectory: true)
        self.remoteConnectionRuntime = remoteConnectionRuntime
        self.pairingState = configuration.initialPairingState
        self.routeStore.replace(
            deviceID: self.pairingState.deviceID,
            workspaceID: self.pairingState.workspaces.first?.id
        )
    }

    public func fetchStatus() async throws -> LocalConnectorStatus {
        if pairingState.deviceID != nil, pairingState.gatewayConnectionEnabled != false, !gatewayConnected {
            try? await connectGateway()
        }
        return try await statusSnapshot()
    }

    public func activateClientStorage(ownerUserID: String) async throws {
        activeClientStorageOwnerID = nil
        pairingState = .empty
        routeStore.replace(deviceID: nil, workspaceID: nil)
        cachedAccessToken = nil
        hasLoadedAccessToken = false
        do {
            let loadedPairingState = try await pairingStateStore.activate(ownerUserID: ownerUserID)
            _ = try await runtimePreferencesStore.activate(ownerUserID: ownerUserID)
            _ = try await approvalStore.activate(ownerUserID: ownerUserID)
            try await pluginStateStore.activate(ownerUserID: ownerUserID)
            pairingState = loadedPairingState
            routeStore.replace(
                deviceID: loadedPairingState.deviceID,
                workspaceID: loadedPairingState.workspaces.first?.id
            )
            activeClientStorageOwnerID = ownerUserID
        } catch {
            await pairingStateStore.deactivate()
            await runtimePreferencesStore.deactivate()
            await approvalStore.deactivate()
            await pluginStateStore.deactivate()
            throw error
        }
    }

    public func deactivateClientStorage() async {
        await stopServerAccess()
        await pluginApplicationRuntime.stopAll()
        await browserExtensionPairingRuntime.stop()
        activeClientStorageOwnerID = nil
        pairingState = .empty
        routeStore.replace(deviceID: nil, workspaceID: nil)
        cachedAccessToken = nil
        hasLoadedAccessToken = false
        await pairingStateStore.deactivate()
        await runtimePreferencesStore.deactivate()
        await approvalStore.deactivate()
        await pluginStateStore.deactivate()
    }

    public func pairWithCurrentChatOSSession(deviceName: String?) async throws -> LocalConnectorStatus {
        invalidateManagedRuntimeConfig()
        let resolvedName = deviceName?.trimmedNonEmpty ?? Host.current().localizedName ?? "Mac"
        let ownerUserID = try activeClientStorageOwnerUserID()
        let ticket = try await ticketProvider.issueLocalConnectorPairingTicket()
        let login = try await gateway.exchange(ticket: ticket, deviceName: resolvedName)
        guard login.user.id == ownerUserID else {
            throw NativeConnectorPairingStateStoreError.accountMismatch
        }
        try secretStore.save(
            Data(login.token.utf8),
            account: Self.accessTokenAccount(ownerUserID: ownerUserID)
        )
        cachedAccessToken = login.token
        hasLoadedAccessToken = true
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
        var next = NativeConnectorPairingState(
            user: login.user.domainModel,
            deviceID: device.id,
            deviceName: resolvedName,
            workspaces: [workspace],
            gatewayConnectionEnabled: true
        )
        next = try await pairingStateStore.save(ownerUserID: ownerUserID, value: next)
        pairingState = next
        routeStore.replace(deviceID: device.id, workspaceID: workspace.id)
        try await connectGateway()
        try? await Task.sleep(for: .milliseconds(200))
        return try await statusSnapshot()
    }

    public func suspendForSignedOut() async {
        await stopServerAccess()
        await pluginApplicationRuntime.stopAll()
        await browserExtensionPairingRuntime.stop()
    }

    public func resumeServerAccess() async throws -> LocalConnectorStatus {
        guard pairingState.deviceID != nil else { throw NativeConnectorError.notPaired }
        let ownerUserID = try activeClientStorageOwnerUserID()
        var next = pairingState
        next.gatewayConnectionEnabled = true
        pairingState = try await pairingStateStore.save(ownerUserID: ownerUserID, value: next)
        try await connectGateway()
        return try await statusSnapshot()
    }

    public func disconnect() async throws -> LocalConnectorStatus {
        let ownerUserID = try activeClientStorageOwnerUserID()
        let token = try? accessToken()
        var next = pairingState
        next.gatewayConnectionEnabled = false
        pairingState = try await pairingStateStore.save(ownerUserID: ownerUserID, value: next)
        await stopServerAccess()
        if let deviceID = pairingState.deviceID, let token {
            try? await gateway.disconnectDevice(token: token, id: deviceID)
        }
        return try await statusSnapshot()
    }

    private func stopServerAccess() async {
        invalidateManagedRuntimeConfig()
        shouldMaintainGatewayConnection = false
        gatewayReconnectFailureCount = 0
        await stopGatewayConnection()
    }

    public func prepareForSystemSleep() async {
        guard pairingState.deviceID != nil else { return }
        isSystemSleeping = true
        reconnectTask?.cancel()
        reconnectTask = nil
        await closeGatewayConnection(terminatePluginSessions: true)
        await pluginApplicationRuntime.stopAll()
        await browserExtensionPairingRuntime.stop()
    }

    public func recoverGatewayConnection(forceReconnect: Bool = false) async {
        guard pairingState.deviceID != nil, pairingState.gatewayConnectionEnabled != false else { return }
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
        let preferences = try await runtimePreferences()
        return runtimeSettingsSnapshot(preferences: preferences)
    }

    public func updateDeveloperMode(_ enabled: Bool) async throws -> LocalConnectorRuntimeSettings {
        let ownerUserID = try activeClientStorageOwnerUserID()
        let preferences = try await runtimePreferencesStore.updateDeveloperMode(
            ownerUserID: ownerUserID,
            enabled: enabled
        )
        return runtimeSettingsSnapshot(preferences: preferences)
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
        guard let workspace = pairingState.workspaces.first(where: { $0.id == workspaceID }) else {
            throw NativeConnectorError.workspaceUnavailable
        }
        let result = try NativeTerminalExecutor.execute(
            command: "/bin/zsh",
            args: ["-lc", commandLine],
            cwd: cwd ?? workspace.absoluteRoot,
            workspace: workspace
        )
        guard let ownerUserID = pairingState.user?.id else { throw NativeConnectorError.notPaired }
        try await terminalHistoryStore.append(
            ownerUserID: ownerUserID,
            entry: .init(
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
                startedAt: ISO8601DateFormatter().string(from: Date())
            )
        )
        return result
    }

    public func fetchCommandHistory(limit: Int) async throws -> [LocalConnectorCommandHistoryEntry] {
        guard let ownerUserID = pairingState.user?.id else { throw NativeConnectorError.notPaired }
        return try await terminalHistoryStore.list(ownerUserID: ownerUserID, limit: limit)
    }

    public func clearCommandHistory() async throws {
        guard let ownerUserID = pairingState.user?.id else { throw NativeConnectorError.notPaired }
        try await terminalHistoryStore.clear(ownerUserID: ownerUserID)
    }

    public func fetchApprovalSettings() async throws -> LocalConnectorApprovalSettings {
        let ownerUserID = try activeClientStorageOwnerUserID()
        async let preferences = approvalStore.preferences(ownerUserID: ownerUserID)
        async let history = approvalStore.history(ownerUserID: ownerUserID)
        return try await .init(defaultMode: preferences.defaultMode, history: history)
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
        let ownerUserID = try activeClientStorageOwnerUserID()
        let current = try await approvalStore.preferences(ownerUserID: ownerUserID)
        if mode == .autoApproval,
           current.commandApprovalModelConfigID?.trimmedNonEmpty == nil {
            throw NativeConnectorError.server(
                status: 409,
                message: "请先在 AI 模型配置中选择本机审批 Agent 模型。"
            )
        }
        let preferences = try await approvalStore.updateMode(ownerUserID: ownerUserID, mode: mode)
        let history = try await approvalStore.history(ownerUserID: ownerUserID)
        return .init(defaultMode: preferences.defaultMode, history: history)
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
        guard let pending = pendingApprovals.first(where: { $0.id == id }) else {
            throw NativeConnectorError.server(status: 404, message: "待审批操作不存在或已经处理。")
        }
        let ownerUserID = try activeClientStorageOwnerUserID()
        let preferences = try await approvalStore.preferences(ownerUserID: ownerUserID)
        let approved = ["accept", "acceptForSession", "approve"].contains(decision)
        let decisionReason = decision == "acceptForSession"
            ? "用户已允许当前会话继续执行此类操作。"
            : (approved ? "用户已允许这次操作。" : "用户已拒绝这次操作。")
        do {
            _ = try await approvalStore.append(
                ownerUserID: ownerUserID,
                entry: .init(
                    id: UUID().uuidString,
                    command: pending.command,
                    cwd: pending.cwd,
                    source: pending.source,
                    mode: preferences.defaultMode,
                    decision: approved ? "approved" : "denied",
                    risk: pending.risk,
                    reason: pending.reason ?? decisionReason,
                    createdAt: ISO8601DateFormatter().string(from: Date())
                )
            )
        } catch {
            pendingApprovals.removeAll(where: { $0.id == id })
            publishApprovalSnapshot()
            let continuation = pendingApprovalContinuations.removeValue(forKey: id)
            pendingApprovalScopeKeys.removeValue(forKey: id)
            let reason = "审批审计保存失败，操作已拒绝：\(error.localizedDescription)"
            publishApprovalEvent(.init(
                requestID: pending.requestID,
                command: pending.command,
                cwd: pending.cwd,
                source: pending.source,
                risk: pending.risk,
                decision: "denied",
                reason: reason,
                mode: preferences.defaultMode,
                reviewer: .policy
            ))
            continuation?.resume(returning: .deny(reason: reason))
            throw error
        }

        pendingApprovals.removeAll(where: { $0.id == id })
        publishApprovalSnapshot()
        let continuation = pendingApprovalContinuations.removeValue(forKey: id)
        let approvalScopeKey = pendingApprovalScopeKeys.removeValue(forKey: id)
        publishApprovalEvent(.init(
            requestID: pending.requestID,
            command: pending.command,
            cwd: pending.cwd,
            source: pending.source,
            risk: pending.risk,
            decision: approved ? "approved" : "denied",
            reason: decisionReason,
            mode: preferences.defaultMode,
            reviewer: .user
        ))
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
        let preferences = try await runtimePreferences()
        return sandboxSettingsSnapshot(preferences: preferences)
    }

    public func updateSandboxSettings(
        enabled: Bool?,
        permissionProfileID: String?,
        approvalPolicy: String?,
        approvalReviewer: String?,
        networkAccess: String?
    ) async throws -> LocalConnectorSandboxSettings {
        let ownerUserID = try activeClientStorageOwnerUserID()
        let preferences = try await runtimePreferencesStore.updateSandbox(
            ownerUserID: ownerUserID,
            enabled: enabled,
            permissionProfileID: permissionProfileID,
            approvalPolicy: approvalPolicy,
            approvalReviewer: approvalReviewer,
            networkAccess: networkAccess
        )
        return sandboxSettingsSnapshot(preferences: preferences)
    }

    private func statusSnapshot() async throws -> LocalConnectorStatus {
        let preferences = try await runtimePreferences()
        return .init(
            configured: pairingState.deviceID != nil && (try? accessToken()) != nil,
            connectorRunning: gatewayConnected,
            developerMode: preferences.developerMode,
            cloudBaseURL: configuration.gatewayBaseURL.absoluteString,
            userServiceBaseURL: configuration.gatewayBaseURL.absoluteString,
            deviceID: pairingState.deviceID,
            deviceName: pairingState.deviceName,
            user: pairingState.user,
            defaultWorkspaceID: pairingState.workspaces.first?.id,
            workspaces: pairingState.workspaces
        )
    }

    private func runtimeSettingsSnapshot(
        preferences: NativeConnectorRuntimePreferences
    ) -> LocalConnectorRuntimeSettings {
        .init(
            developerMode: preferences.developerMode,
            developerCloudBaseURL: configuration.gatewayBaseURL.absoluteString,
            developerUserServiceBaseURL: configuration.gatewayBaseURL.absoluteString,
            developerChatOSWebURL: ""
        )
    }

    private func sandboxSettingsSnapshot(
        preferences: NativeConnectorRuntimePreferences
    ) -> LocalConnectorSandboxSettings {
        .init(
            enabled: preferences.sandboxEnabled,
            defaultBackend: "native-macos",
            defaultPermissionProfileID: preferences.permissionProfileID,
            defaultPermissionProfileName: preferences.permissionProfileID,
            defaultApprovalPolicy: preferences.approvalPolicy,
            defaultApprovalReviewer: preferences.approvalReviewer,
            defaultNetworkAccess: preferences.networkAccess,
            permissionConfigurationError: nil,
            policyRevision: preferences.policyRevision
        )
    }

    func activeClientStorageOwnerUserID() throws -> String {
        guard let ownerUserID = activeClientStorageOwnerID else {
            throw NativeLocalClientSettingStoreError.notLoaded
        }
        return ownerUserID
    }

    private func runtimePreferences() async throws -> NativeConnectorRuntimePreferences {
        let ownerUserID = try activeClientStorageOwnerUserID()
        return try await runtimePreferencesStore.value(ownerUserID: ownerUserID)
    }

    private func ensureDefaultWorkspace(
        token: String,
        deviceID: String,
        publicKey: String
    ) async throws -> LocalConnectorWorkspace {
        let root = "/"
        let digest = SHA256.hash(data: Data("\(root)\u{0}\(publicKey)".utf8))
        let fingerprint = digest.map { String(format: "%02x", $0) }.joined()
        let existing = try await gateway.listWorkspaces(token: token).first {
            $0.deviceID == deviceID && $0.localPathFingerprint == fingerprint
        }
        let remote: GatewayWorkspaceDTO
        if let existing {
            remote = existing
        } else {
            remote = try await gateway.createWorkspace(
                token: token,
                deviceID: deviceID,
                alias: "本机文件系统",
                fingerprint: fingerprint
            )
        }
        return .init(
            id: remote.id,
            alias: remote.localPathAlias,
            absoluteRoot: root,
            fingerprint: remote.localPathFingerprint
        )
    }

    private func connectGateway() async throws {
        shouldMaintainGatewayConnection = true
        guard webSocket == nil,
              reconnectTask == nil,
              gatewayConnectionCleanupCount == 0 else {
            return
        }
        do {
            try await openGatewayConnection()
        } catch {
            recordGatewayReconnectFailure()
            scheduleGatewayReconnect()
            throw error
        }
    }

    private func openGatewayConnection() async throws {
        invalidateManagedRuntimeConfig()
        let token = try requireAccessToken()
        guard let deviceID = pairingState.deviceID else { throw NativeConnectorError.notPaired }
        let identity = try deviceIdentity()
        let path = "/api/local-connectors/devices/\(deviceID)/connect"
        let base = configuration.gatewayBaseURL.absoluteString
            .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        guard var components = URLComponents(string: base + path) else {
            throw NativeConnectorError.invalidEndpoint
        }
        components.scheme = components.scheme == "https" ? "wss" : "ws"
        guard let url = components.url else { throw NativeConnectorError.invalidEndpoint }
        let timestamp = String(Int(Date().timeIntervalSince1970))
        let nonce = UUID().uuidString
        let payload = NativeConnectorDeviceAuthentication.connectionPayload(
            deviceID: deviceID,
            timestamp: timestamp,
            nonce: nonce,
            path: path
        )
        var request = URLRequest(url: url)
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue(deviceID, forHTTPHeaderField: "x-local-connector-device-id")
        request.setValue(timestamp, forHTTPHeaderField: "x-local-connector-device-timestamp")
        request.setValue(nonce, forHTTPHeaderField: "x-local-connector-device-nonce")
        request.setValue(try identity.signature(for: payload), forHTTPHeaderField: "x-local-connector-device-signature")
        request.setValue("ed25519", forHTTPHeaderField: "x-local-connector-device-signature-alg")
        let socket = URLSession.shared.webSocketTask(with: request)
        webSocket = socket
        gatewayConnected = false
        lastGatewayPongAt = nil
        socket.resume()
        receiveTask = Task { [weak self] in await self?.receiveMessages(from: socket) }
        heartbeatTask = Task { [weak self] in await self?.sendHeartbeats(to: socket) }
    }

    private func receiveMessages(from socket: URLSessionWebSocketTask) async {
        do {
            while webSocket === socket {
                let message = try await socket.receive()
                switch message {
                case let .string(text):
                    if let data = text.data(using: .utf8),
                       let envelope = try? JSONDecoder().decode(GatewaySocketEnvelope.self, from: data) {
                        switch envelope.type {
                        case "connected":
                            gatewayConnected = true
                            lastGatewayPongAt = Date()
                            gatewayReconnectFailureCount = 0
                            Self.logger.info("Local Connector 网关长连接已建立")
                            Task { [weak self] in
                                _ = try? await self?.managedRuntimeConfig()
                            }
                            Task { [weak self] in
                                try? await self?.publishPluginInstallationStatus()
                            }
                        case "pong":
                            lastGatewayPongAt = Date()
                        case "error":
                            throw NativeConnectorError.server(
                                status: 503,
                                message: envelope.message
                                    ?? envelope.code
                                    ?? "Local Connector 网关会话异常"
                            )
                        case "terminal_exec_request":
                            Task { [weak self] in
                                await self?.handleTerminalRelayMessage(data, socket: socket)
                            }
                        case "mcp":
                            Task { [weak self] in
                                await self?.handleMCPRelayMessage(data, socket: socket)
                            }
                        case "plugin_prepare_request",
                             "plugin_execute_request",
                             "plugin_cancel_request":
                            Task { [weak self] in
                                await self?.handlePluginRelayMessage(data, socket: socket)
                            }
                        case "workspace_directory_list_request",
                             "workspace_directory_create_request",
                             "workspace_filesystem_request":
                            Task { [weak self] in
                                await self?.handleWorkspaceRelayMessage(data, socket: socket)
                            }
                        default:
                            break
                        }
                    }
                case .data:
                    break
                @unknown default:
                    break
                }
            }
        } catch {
            await handleGatewayConnectionFailure(socket: socket, error: error)
        }
    }

    private func sendHeartbeats(to socket: URLSessionWebSocketTask) async {
        var missedAcknowledgements = 0
        while !Task.isCancelled, webSocket === socket {
            let sentAt = Date()
            do {
                try await socket.send(.string("{\"type\":\"heartbeat\"}"))
                try await Task.sleep(for: .seconds(15))
            } catch is CancellationError {
                return
            } catch {
                await handleGatewayConnectionFailure(socket: socket, error: error)
                return
            }
            guard webSocket === socket else { return }
            if let lastGatewayPongAt, lastGatewayPongAt >= sentAt {
                missedAcknowledgements = 0
            } else {
                missedAcknowledgements += 1
            }
            if missedAcknowledgements >= 3 {
                await handleGatewayConnectionFailure(
                    socket: socket,
                    error: URLError(.timedOut)
                )
                return
            }
        }
    }

    private func handleGatewayConnectionFailure(
        socket: URLSessionWebSocketTask,
        error: any Error
    ) async {
        guard webSocket === socket else { return }
        let authenticationExpired = NativeConnectorGateway.isConnectorAuthenticationRejected(
            statusCode: (socket.response as? HTTPURLResponse)?.statusCode ?? 0,
            token: (try? accessToken()) ?? nil
        )
        if authenticationExpired {
            shouldMaintainGatewayConnection = false
            invalidateManagedRuntimeConfig()
        }
        Self.logger.error("网关长连接中断：\(error.localizedDescription, privacy: .public)")
        recordGatewayReconnectFailure()
        // A transient gateway outage must not destroy prepared Plugin MCP
        // processes. Their adapter session IDs remain valid locally and can be
        // reused after the authenticated connector channel is re-established.
        await closeGatewayConnection(
            terminatePluginSessions: Self.transientGatewayFailureTerminatesPluginSessions
        )
        if !authenticationExpired {
            scheduleGatewayReconnect()
        }
    }

    static let transientGatewayFailureTerminatesPluginSessions = false

    private func scheduleGatewayReconnect() {
        guard shouldMaintainGatewayConnection,
              !isSystemSleeping,
              pairingState.deviceID != nil,
              webSocket == nil,
              gatewayConnectionCleanupCount == 0,
              reconnectTask == nil else {
            return
        }
        reconnectTask = Task { [weak self] in
            await self?.runGatewayReconnectLoop()
        }
    }

    private func runGatewayReconnectLoop() async {
        defer {
            reconnectTask = nil
            if shouldMaintainGatewayConnection,
               !isSystemSleeping,
               pairingState.deviceID != nil,
               webSocket == nil {
                scheduleGatewayReconnect()
            }
        }
        while !Task.isCancelled,
              shouldMaintainGatewayConnection,
              !isSystemSleeping,
              pairingState.deviceID != nil,
              webSocket == nil {
            let delay = Self.gatewayReconnectDelaySeconds(
                afterFailedAttempts: gatewayReconnectFailureCount
            )
            if delay > 0 {
                Self.logger.info("将在 \(delay, privacy: .public) 秒后重连 Local Connector 网关")
                do {
                    try await Task.sleep(for: .seconds(delay))
                } catch {
                    return
                }
            }
            do {
                try await openGatewayConnection()
                Self.logger.info("已发起 Local Connector 网关重连")
                return
            } catch {
                recordGatewayReconnectFailure()
                Self.logger.error(
                    "网关重连失败，将继续重试：\(error.localizedDescription, privacy: .public)"
                )
            }
        }
    }

    static func gatewayReconnectDelaySeconds(afterFailedAttempts attempts: Int) -> Int {
        guard attempts > 0 else { return 0 }
        return min(30, 1 << min(attempts - 1, 5))
    }

    private func recordGatewayReconnectFailure() {
        gatewayReconnectFailureCount = min(gatewayReconnectFailureCount + 1, 6)
    }

    func publishPluginInstallationStatus() async throws {
        _ = try requireAccessToken()
        try await sendPluginInstallationStatus()
    }

    func sendPluginInstallationStatus() async throws {
        guard let socket = webSocket,
              let ownerUserID = pairingState.user?.id,
              let deviceID = pairingState.deviceID else {
            return
        }
        let installations = try await pluginStateStore.installations(ownerUserID: ownerUserID)
        let items = installations.values
            .sorted { $0.record.pluginID < $1.record.pluginID }
            .compactMap { installation in
                let record = installation.record
                return try? NativePluginInstallationStatusBuilder.makeItem(
                    record: record,
                    ownerUserID: ownerUserID,
                    deviceID: deviceID,
                    platform: Self.pluginPlatform,
                    active: installation.enabled
                )
            }
        let data = try JSONEncoder().encode(
            GatewayPluginInstallationStatusMessage(
                type: "plugin_installation_status",
                items: items
            )
        )
        guard let text = String(data: data, encoding: .utf8) else {
            throw NativeConnectorError.invalidResponse("无法编码 Plugin 安装状态")
        }
        try await socket.send(.string(text))
    }

    private static var pluginPlatform: String {
#if arch(arm64)
        "macos-arm64"
#else
        "macos-x64"
#endif
    }

    private func stopGatewayConnection() async {
        reconnectTask?.cancel()
        reconnectTask = nil
        await closeGatewayConnection(terminatePluginSessions: true)
    }

    private func closeGatewayConnection(terminatePluginSessions: Bool) async {
        gatewayConnectionCleanupCount += 1
        defer {
            gatewayConnectionCleanupCount -= 1
            if gatewayConnectionCleanupCount == 0,
               shouldMaintainGatewayConnection,
               !isSystemSleeping,
               pairingState.deviceID != nil,
               webSocket == nil {
                scheduleGatewayReconnect()
            }
        }
        let continuations = pendingApprovalContinuations.values
        pendingApprovalContinuations.removeAll()
        pendingApprovalScopeKeys.removeAll()
        sessionApprovalAllowlist.removeAll()
        pendingApprovals.removeAll()
        publishApprovalSnapshot()
        for continuation in continuations {
            continuation.resume(returning: .deny(reason: "本机连接器已断开。"))
        }
        receiveTask?.cancel()
        heartbeatTask?.cancel()
        receiveTask = nil
        heartbeatTask = nil
        webSocket?.cancel(with: .goingAway, reason: nil)
        webSocket = nil
        gatewayConnected = false
        lastGatewayPongAt = nil
        if terminatePluginSessions {
            await pluginRuntimeStore.terminateAll()
            pluginSkillRuntimeSessions.removeAll()
        }
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

    private func accessToken() throws -> String? {
        if hasLoadedAccessToken { return cachedAccessToken }
        let ownerUserID = try activeClientStorageOwnerUserID()
        guard let data = try secretStore.load(
            account: Self.accessTokenAccount(ownerUserID: ownerUserID)
        ) else {
            cachedAccessToken = nil
            hasLoadedAccessToken = true
            return nil
        }
        cachedAccessToken = String(data: data, encoding: .utf8)?.trimmedNonEmpty
        hasLoadedAccessToken = true
        return cachedAccessToken
    }

    private static func accessTokenAccount(ownerUserID: String) -> String {
        "gateway-access-token-v1:\(ownerUserID)"
    }

    func requireAccessToken() throws -> String {
        guard let token = try accessToken() else { throw NativeConnectorError.notPaired }
        return token
    }

    private func deviceIdentity() throws -> NativeConnectorDeviceIdentity {
        if let cachedDeviceIdentity { return cachedDeviceIdentity }
        let identity = try NativeConnectorDeviceIdentity(secretStore: secretStore)
        cachedDeviceIdentity = identity
        return identity
    }
}

private struct GatewaySocketEnvelope: Decodable {
    var type: String
    var code: String?
    var message: String?
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
