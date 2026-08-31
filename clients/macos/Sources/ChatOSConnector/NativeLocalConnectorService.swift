import ChatOSCore
import CryptoKit
import Foundation
import OSLog

public struct NativeConnectorConfiguration: Sendable {
    public var gatewayBaseURL: URL
    public var stateURL: URL

    public init(gatewayBaseURL: URL, stateURL: URL) {
        self.gatewayBaseURL = gatewayBaseURL
        self.stateURL = stateURL
    }
}

public actor NativeLocalConnectorService: LocalConnectorControlServicing, LocalConnectorApprovalStreaming {
    private static let accessTokenAccount = "gateway-access-token-v1"
    private static let logger = Logger(
        subsystem: "com.chatos.swift-client",
        category: "NativeLocalConnector"
    )

    private let configuration: NativeConnectorConfiguration
    private let ticketProvider: any LocalConnectorPairingTicketProviding
    let gateway: NativeConnectorGateway
    let stateStore: NativeConnectorStateStore
    let pluginInstaller: NativePluginInstaller
    let mcpCodeWriteStore = NativeMCPCodeWriteStore()
    let mcpTerminalStore = NativeMCPTerminalStore()
    let pluginRuntimeStore = NativePluginRuntimeStore()
    let pluginRuntimeRootURL: URL
    let remoteConnectionRuntime: (any NativeRemoteConnectionRuntimeProviding)?
    private let secretStore = NativeConnectorSecretStore()
    var state: NativeConnectorPersistentState
    private var cachedAccessToken: String?
    private var hasLoadedAccessToken = false
    private var cachedDeviceIdentity: NativeConnectorDeviceIdentity?
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
        remoteConnectionRuntime: (any NativeRemoteConnectionRuntimeProviding)? = nil
    ) {
        self.configuration = configuration
        self.ticketProvider = ticketProvider
        self.gateway = NativeConnectorGateway(baseURL: configuration.gatewayBaseURL)
        self.stateStore = NativeConnectorStateStore(stateURL: configuration.stateURL)
        self.pluginInstaller = NativePluginInstaller(
            rootURL: configuration.stateURL
                .deletingLastPathComponent()
                .appendingPathComponent("Plugins", isDirectory: true)
        )
        self.pluginRuntimeRootURL = configuration.stateURL
            .deletingLastPathComponent()
            .appendingPathComponent("PluginRuntime", isDirectory: true)
        self.remoteConnectionRuntime = remoteConnectionRuntime
        self.state = (try? stateStore.load()) ?? .empty
    }

    public func fetchStatus() async throws -> LocalConnectorStatus {
        if state.deviceID != nil {
            try? await importLegacyWorkspacesIfNeeded()
        }
        if state.deviceID != nil, !gatewayConnected {
            try? await connectGateway()
        }
        return statusSnapshot()
    }

    public func pairWithCurrentChatOSSession(deviceName: String?) async throws -> LocalConnectorStatus {
        let resolvedName = deviceName?.trimmedNonEmpty ?? Host.current().localizedName ?? "Mac"
        let ticket = try await ticketProvider.issueLocalConnectorPairingTicket()
        let login = try await gateway.exchange(ticket: ticket, deviceName: resolvedName)
        try secretStore.save(Data(login.token.utf8), account: Self.accessTokenAccount)
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
        state.user = login.user.domainModel
        state.deviceID = device.id
        state.deviceName = resolvedName
        state.workspaces = [workspace]
        try? await importLegacyWorkspacesIfNeeded()
        try stateStore.save(state)
        try await connectGateway()
        try? await Task.sleep(for: .milliseconds(200))
        return statusSnapshot()
    }

    public func disconnect() async throws -> LocalConnectorStatus {
        shouldMaintainGatewayConnection = false
        gatewayReconnectFailureCount = 0
        let token = try accessToken()
        if let deviceID = state.deviceID, let token {
            try? await gateway.disconnectDevice(token: token, id: deviceID)
        }
        await stopGatewayConnection()
        try secretStore.delete(account: Self.accessTokenAccount)
        cachedAccessToken = nil
        hasLoadedAccessToken = true
        state.user = nil
        state.deviceID = nil
        state.deviceName = nil
        state.workspaces = []
        try stateStore.save(state)
        return statusSnapshot()
    }

    public func prepareForSystemSleep() async {
        guard state.deviceID != nil else { return }
        isSystemSleeping = true
        reconnectTask?.cancel()
        reconnectTask = nil
        await closeGatewayConnection(terminatePluginSessions: true)
    }

    public func recoverGatewayConnection(forceReconnect: Bool = false) async {
        guard state.deviceID != nil else { return }
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
            configured: state.deviceID != nil && (try? accessToken()) != nil,
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
        let token = try requireAccessToken()
        guard let deviceID = state.deviceID else { throw NativeConnectorError.notPaired }
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
                            try? await publishPluginInstallationStatus()
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
        Self.logger.error("网关长连接中断：\(error.localizedDescription, privacy: .public)")
        recordGatewayReconnectFailure()
        await closeGatewayConnection(terminatePluginSessions: true)
        scheduleGatewayReconnect()
    }

    private func scheduleGatewayReconnect() {
        guard shouldMaintainGatewayConnection,
              !isSystemSleeping,
              state.deviceID != nil,
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
               state.deviceID != nil,
               webSocket == nil {
                scheduleGatewayReconnect()
            }
        }
        while !Task.isCancelled,
              shouldMaintainGatewayConnection,
              !isSystemSleeping,
              state.deviceID != nil,
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
        guard let socket = webSocket,
              let ownerUserID = state.user?.id,
              let deviceID = state.deviceID else {
            return
        }
        let records = state.installedPluginRecords ?? [:]
        let items = records.values
            .sorted { $0.pluginID < $1.pluginID }
            .compactMap { record in
                try? NativePluginInstallationStatusBuilder.makeItem(
                    record: record,
                    ownerUserID: ownerUserID,
                    deviceID: deviceID,
                    platform: Self.pluginPlatform,
                    active: state.pluginPreferences[record.pluginID] ?? true
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
               state.deviceID != nil,
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
