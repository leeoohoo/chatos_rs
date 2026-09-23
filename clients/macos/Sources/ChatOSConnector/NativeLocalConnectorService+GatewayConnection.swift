import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import OSLog

extension NativeLocalConnectorService {
    func ensureDefaultWorkspace(
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

    func connectGateway() async throws {
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
                        case "terminal_exec_request",
                             "terminal_session_create_request",
                             "terminal_input",
                             "terminal_command",
                             "terminal_resize",
                             "terminal_snapshot_request",
                             "terminal_close",
                             "remote_terminal_session_create_request",
                             "remote_terminal_input",
                             "remote_terminal_resize",
                             "remote_terminal_snapshot_request",
                             "remote_terminal_close":
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
                        case let messageType where Self.isCompanionRelayMessageType(messageType):
                            Task { [weak self] in
                                await self?.handleCompanionRelayMessage(data, socket: socket)
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
        if authenticationExpired {
            await refreshConnectorCredentialAfterRejection()
        } else {
            scheduleGatewayReconnect()
        }
    }

    private func refreshConnectorCredentialAfterRejection() async {
        let now = Date()
        guard Self.shouldAttemptConnectorCredentialRefresh(
            lastAttempt: lastConnectorCredentialRefreshAttemptAt,
            now: now
        ) else {
            Self.logger.error("Connector 凭证刚刚续签过但仍被拒绝，已停止自动重试")
            return
        }
        lastConnectorCredentialRefreshAttemptAt = now
        guard state.gatewayConnectionEnabled != false,
              let expectedOwnerUserID = state.user?.id.gatewayTrimmedNonEmpty,
              let deviceName = state.deviceName?.gatewayTrimmedNonEmpty else {
            Self.logger.error("Connector 凭证已失效，但本机缺少安全续签所需的配对信息")
            return
        }
        do {
            let ticket = try await ticketProvider.issueLocalConnectorPairingTicket()
            let login = try await gateway.exchange(ticket: ticket, deviceName: deviceName)
            guard login.user.id == expectedOwnerUserID else {
                throw NativeConnectorError.server(
                    status: 409,
                    message: "当前登录账号与已配对设备账号不一致"
                )
            }
            try secretStore.save(Data(login.token.utf8), account: Self.accessTokenAccount)
            cachedAccessToken = login.token
            hasLoadedAccessToken = true
            state.user = login.user.domainModel
            try stateStore.save(state)
            shouldMaintainGatewayConnection = true
            gatewayReconnectFailureCount = 0
            Self.logger.info("Connector 凭证已自动续签，正在恢复网关长连接")
            scheduleGatewayReconnect()
        } catch {
            shouldMaintainGatewayConnection = false
            Self.logger.error(
                "Connector 凭证自动续签失败：\(error.localizedDescription, privacy: .public)"
            )
        }
    }

    static let transientGatewayFailureTerminatesPluginSessions = false

    func scheduleGatewayReconnect() {
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

    static let connectorCredentialRefreshCooldown: TimeInterval = 60

    static func shouldAttemptConnectorCredentialRefresh(
        lastAttempt: Date?,
        now: Date
    ) -> Bool {
        guard let lastAttempt else { return true }
        return now.timeIntervalSince(lastAttempt) >= connectorCredentialRefreshCooldown
    }

    private func recordGatewayReconnectFailure() {
        gatewayReconnectFailureCount = min(gatewayReconnectFailureCount + 1, 6)
    }

    func publishPluginInstallationStatus() async throws {
        let token = try requireAccessToken()
        let sources = try await gateway.pluginSources(token: token)
        if reconcileInstalledPluginIdentities(with: sources.items) {
            try stateStore.save(state)
        }
        try await sendPluginInstallationStatus()
    }

    func sendPluginInstallationStatus() async throws {
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

    func stopGatewayConnection() async {
        reconnectTask?.cancel()
        reconnectTask = nil
        await closeGatewayConnection(terminatePluginSessions: true)
    }

    func closeGatewayConnection(terminatePluginSessions: Bool) async {
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
            pluginSkillRuntimeSessions.removeAll()
        }
    }
}

private struct GatewaySocketEnvelope: Decodable {
    var type: String
    var code: String?
    var message: String?
}

private extension String {
    var gatewayTrimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
