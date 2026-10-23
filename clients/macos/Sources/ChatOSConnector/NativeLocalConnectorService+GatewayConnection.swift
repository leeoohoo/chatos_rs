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
        guard Self.shouldOpenGatewayConnection(
            hasSocket: webSocket != nil,
            hasReconnectTask: reconnectTask != nil,
            hasFailureHandler: gatewayConnectionFailureTask != nil,
            cleanupInProgress: gatewayConnectionCleanupCount > 0
        ) else {
            return
        }
        shouldMaintainGatewayConnection = true
        do {
            try await openGatewayConnection()
        } catch {
            recordGatewayReconnectFailure()
            scheduleGatewayReconnect()
            throw error
        }
    }

    static func shouldOpenGatewayConnection(
        hasSocket: Bool,
        hasReconnectTask: Bool,
        hasFailureHandler: Bool,
        cleanupInProgress: Bool
    ) -> Bool {
        !hasSocket && !hasReconnectTask && !hasFailureHandler && !cleanupInProgress
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
                            startGatewayConnectedBootstrap(for: socket)
                        case "pong":
                            lastGatewayPongAt = Date()
                        case "error":
                            if Self.isRecoverableGatewayProtocolError(envelope.code) {
                                Self.logger.warning(
                                    "网关拒绝了一条控制消息但连接保持：\((envelope.message ?? envelope.code ?? "unknown"), privacy: .public)"
                                )
                                continue
                            }
                            throw NativeConnectorError.server(
                                status: 503,
                                message: envelope.message
                                    ?? envelope.code
                                    ?? "Local Connector 网关会话异常"
                            )
                        case let messageType where Self.isCompanionRelayMessageType(messageType):
                            await enqueueCompanionRelayMessage(data, socket: socket)
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
            scheduleGatewayConnectionFailure(socket: socket, error: error)
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
                scheduleGatewayConnectionFailure(socket: socket, error: error)
                return
            }
            guard webSocket === socket else { return }
            if let lastGatewayPongAt, lastGatewayPongAt >= sentAt {
                missedAcknowledgements = 0
            } else {
                missedAcknowledgements += 1
            }
            if missedAcknowledgements >= 3 {
                scheduleGatewayConnectionFailure(
                    socket: socket,
                    error: URLError(.timedOut)
                )
                return
            }
        }
    }

    /// Failure recovery runs outside the socket reader and heartbeat tasks. Closing a
    /// connection intentionally cancels both of those tasks; doing the ticket exchange from
    /// either one caused the replacement request to inherit cancellation and fail immediately.
    private func scheduleGatewayConnectionFailure(
        socket: URLSessionWebSocketTask,
        error: any Error
    ) {
        guard Self.shouldStartGatewayConnectionFailureHandling(
            socketMatches: webSocket === socket,
            hasActiveHandler: gatewayConnectionFailureTask != nil,
            cleanupInProgress: gatewayConnectionCleanupCount > 0
        ) else { return }
        gatewayConnectionFailureGeneration &+= 1
        let generation = gatewayConnectionFailureGeneration
        gatewayConnectionFailureTask = Task { [weak self] in
            await self?.handleGatewayConnectionFailure(
                socket: socket,
                error: error,
                generation: generation
            )
            await self?.finishGatewayConnectionFailureHandling(generation: generation)
        }
    }

    private func finishGatewayConnectionFailureHandling(generation: UInt64) {
        guard gatewayConnectionFailureGeneration == generation else { return }
        gatewayConnectionFailureTask = nil
        scheduleGatewayReconnect()
    }

    static func shouldStartGatewayConnectionFailureHandling(
        socketMatches: Bool,
        hasActiveHandler: Bool,
        cleanupInProgress: Bool
    ) -> Bool {
        socketMatches && !hasActiveHandler && !cleanupInProgress
    }

    private func handleGatewayConnectionFailure(
        socket: URLSessionWebSocketTask,
        error: any Error,
        generation: UInt64
    ) async {
        guard Self.gatewayConnectionFailureIsCurrent(
            expectedGeneration: generation,
            currentGeneration: gatewayConnectionFailureGeneration,
            isCancelled: Task.isCancelled,
            socketMatches: webSocket === socket
        ) else { return }
        let statusCode = (socket.response as? HTTPURLResponse)?.statusCode ?? 0
        let authenticationExpired = NativeConnectorGateway.isConnectorAuthenticationRejected(
            statusCode: statusCode,
            token: (try? accessToken()) ?? nil
        )
        if authenticationExpired {
            shouldMaintainGatewayConnection = false
            invalidateManagedRuntimeConfig()
        }
        Self.logger.error(
            "网关长连接中断（HTTP \(statusCode, privacy: .public)）：\(error.localizedDescription, privacy: .public)"
        )
        recordGatewayReconnectFailure()
        // A transient gateway outage must not destroy prepared Plugin MCP
        // processes. Their adapter session IDs remain valid locally and can be
        // reused after the authenticated connector channel is re-established.
        await closeGatewayConnection(
            terminatePluginSessions: Self.transientGatewayFailureTerminatesPluginSessions
        )
        guard Self.gatewayConnectionFailureIsCurrent(
            expectedGeneration: generation,
            currentGeneration: gatewayConnectionFailureGeneration,
            isCancelled: Task.isCancelled,
            socketMatches: webSocket == nil
        ) else { return }
        if authenticationExpired {
            await refreshConnectorCredentialAfterRejection(generation: generation)
        } else {
            scheduleGatewayReconnect()
        }
    }

    private func refreshConnectorCredentialAfterRejection(generation: UInt64) async {
        // Status polling may call connectGateway while this actor is suspended in the
        // ticket exchange. Keep the old credential disabled until the replacement has
        // been verified and durably stored.
        shouldMaintainGatewayConnection = false
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
            try requireCurrentGatewayConnectionFailure(generation: generation)
            let ticket = try await ticketProvider.issueLocalConnectorPairingTicket()
            try requireCurrentGatewayConnectionFailure(generation: generation)
            let login = try await gateway.exchange(ticket: ticket, deviceName: deviceName)
            try requireCurrentGatewayConnectionFailure(generation: generation)
            guard login.user.id == expectedOwnerUserID else {
                throw NativeConnectorError.server(
                    status: 409,
                    message: "当前登录账号与已配对设备账号不一致"
                )
            }
            // Status polling can re-enter the actor while the ticket exchange is in flight.
            // Remove any old-token socket it managed to start before publishing the new token.
            shouldMaintainGatewayConnection = false
            if webSocket != nil {
                await closeGatewayConnection(terminatePluginSessions: false)
            }
            try requireCurrentGatewayConnectionFailure(generation: generation)
            try secretStore.save(Data(login.token.utf8), account: Self.accessTokenAccount)
            cachedAccessToken = login.token
            hasLoadedAccessToken = true
            state.user = login.user.domainModel
            try stateStore.save(state)
            scheduleConnectorCredentialRenewal(for: login.token)
            shouldMaintainGatewayConnection = true
            gatewayReconnectFailureCount = 0
            Self.logger.info("Connector 凭证已自动续签，正在恢复网关长连接")
            scheduleGatewayReconnect()
        } catch is CancellationError {
            return
        } catch {
            shouldMaintainGatewayConnection = false
            Self.logger.error(
                "Connector 凭证自动续签失败：\(error.localizedDescription, privacy: .public)"
            )
        }
    }

    func scheduleConnectorCredentialRenewal(
        for token: String,
        now: Date = Date()
    ) {
        connectorCredentialRenewalTask?.cancel()
        let delay = Self.connectorCredentialRenewalDelay(token: token, now: now)
        connectorCredentialRenewalTask = Task { [weak self] in
            do {
                try await Task.sleep(for: .seconds(delay))
            } catch {
                return
            }
            guard !Task.isCancelled else { return }
            await self?.renewConnectorCredentialProactively()
        }
    }

    private func renewConnectorCredentialProactively() async {
        // Clear the current task before pairing so the successful pairing can install the next
        // renewal timer without cancelling the task that is performing the renewal.
        connectorCredentialRenewalTask = nil
        guard state.gatewayConnectionEnabled != false,
              state.user?.id.gatewayTrimmedNonEmpty != nil else { return }
        do {
            _ = try await pairWithCurrentChatOSSession(deviceName: state.deviceName)
            Self.logger.info("Connector 凭证已在过期前主动续签")
        } catch is CancellationError {
            return
        } catch {
            Self.logger.error(
                "Connector 凭证主动续签失败，将稍后重试：\(error.localizedDescription, privacy: .public)"
            )
            scheduleConnectorCredentialRenewalRetry()
        }
    }

    private func scheduleConnectorCredentialRenewalRetry() {
        connectorCredentialRenewalTask?.cancel()
        connectorCredentialRenewalTask = Task { [weak self] in
            do {
                try await Task.sleep(for: .seconds(60))
            } catch {
                return
            }
            guard !Task.isCancelled else { return }
            await self?.renewConnectorCredentialProactively()
        }
    }

    static func connectorCredentialRenewalDelay(
        token: String,
        now: Date
    ) -> TimeInterval {
        struct Claims: Decodable {
            let iat: TimeInterval
            let exp: TimeInterval
        }

        let parts = token.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 3 else { return 60 * 60 }
        var payload = String(parts[1])
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        payload.append(String(repeating: "=", count: (4 - payload.count % 4) % 4))
        guard let data = Data(base64Encoded: payload),
              let claims = try? JSONDecoder().decode(Claims.self, from: data),
              claims.exp > claims.iat else {
            return 60 * 60
        }
        let issuedLifetime = claims.exp - claims.iat
        let renewalAt = claims.iat + issuedLifetime / 2
        return max(0, renewalAt - now.timeIntervalSince1970)
    }

    private func requireCurrentGatewayConnectionFailure(generation: UInt64) throws {
        guard Self.gatewayConnectionFailureIsCurrent(
            expectedGeneration: generation,
            currentGeneration: gatewayConnectionFailureGeneration,
            isCancelled: Task.isCancelled,
            socketMatches: true
        ) else {
            throw CancellationError()
        }
    }

    static func gatewayConnectionFailureIsCurrent(
        expectedGeneration: UInt64,
        currentGeneration: UInt64,
        isCancelled: Bool,
        socketMatches: Bool
    ) -> Bool {
        !isCancelled && expectedGeneration == currentGeneration && socketMatches
    }

    static let transientGatewayFailureTerminatesPluginSessions = false

    static func isRecoverableGatewayProtocolError(_ code: String?) -> Bool {
        switch code {
        case "plugin_installation_status_rejected",
             "plugin_oauth_status_rejected",
             "invalid_relay_response":
            true
        default:
            false
        }
    }

    func scheduleGatewayReconnect() {
        guard Self.shouldScheduleGatewayReconnect(
            shouldMaintain: shouldMaintainGatewayConnection,
            isSystemSleeping: isSystemSleeping,
            hasDevice: state.deviceID != nil,
            hasSocket: webSocket != nil,
            cleanupInProgress: gatewayConnectionCleanupCount > 0,
            hasFailureHandler: gatewayConnectionFailureTask != nil,
            hasReconnectTask: reconnectTask != nil
        ) else {
            return
        }
        reconnectTask = Task { [weak self] in
            await self?.runGatewayReconnectLoop()
        }
    }

    static func shouldScheduleGatewayReconnect(
        shouldMaintain: Bool,
        isSystemSleeping: Bool,
        hasDevice: Bool,
        hasSocket: Bool,
        cleanupInProgress: Bool,
        hasFailureHandler: Bool,
        hasReconnectTask: Bool
    ) -> Bool {
        shouldMaintain
            && !isSystemSleeping
            && hasDevice
            && !hasSocket
            && !cleanupInProgress
            && !hasFailureHandler
            && !hasReconnectTask
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

    private func startGatewayConnectedBootstrap(for socket: URLSessionWebSocketTask) {
        gatewayConnectedBootstrapGeneration &+= 1
        let generation = gatewayConnectedBootstrapGeneration
        gatewayConnectedBootstrapTask?.cancel()
        gatewayConnectedBootstrapTask = Task { [weak self] in
            await self?.runGatewayConnectedBootstrap(for: socket, generation: generation)
            await self?.finishGatewayConnectedBootstrap(generation: generation)
        }
    }

    private func runGatewayConnectedBootstrap(
        for socket: URLSessionWebSocketTask,
        generation: UInt64
    ) async {
        guard isGatewayConnectedBootstrapCurrent(socket: socket, generation: generation) else {
            return
        }
        async let runtimeConfig: Void = refreshManagedRuntimeConfig(
            for: socket,
            generation: generation
        )
        async let pluginStatus: Void = refreshPluginInstallationStatus(
            for: socket,
            generation: generation
        )
        _ = await (runtimeConfig, pluginStatus)
    }

    private func refreshManagedRuntimeConfig(
        for socket: URLSessionWebSocketTask,
        generation: UInt64
    ) async {
        guard isGatewayConnectedBootstrapCurrent(socket: socket, generation: generation) else {
            return
        }
        _ = try? await managedRuntimeConfig()
    }

    private func refreshPluginInstallationStatus(
        for socket: URLSessionWebSocketTask,
        generation: UInt64
    ) async {
        guard isGatewayConnectedBootstrapCurrent(socket: socket, generation: generation) else {
            return
        }
        try? await publishPluginInstallationStatus(
            expectedSocket: socket,
            bootstrapGeneration: generation
        )
    }

    private func finishGatewayConnectedBootstrap(generation: UInt64) {
        guard gatewayConnectedBootstrapGeneration == generation else { return }
        gatewayConnectedBootstrapTask = nil
    }

    private func isGatewayConnectedBootstrapCurrent(
        socket: URLSessionWebSocketTask,
        generation: UInt64
    ) -> Bool {
        Self.gatewayConnectedBootstrapIsCurrent(
            expectedGeneration: generation,
            currentGeneration: gatewayConnectedBootstrapGeneration,
            isCancelled: Task.isCancelled,
            socketMatches: webSocket === socket,
            gatewayConnected: gatewayConnected
        )
    }

    static func gatewayConnectedBootstrapIsCurrent(
        expectedGeneration: UInt64,
        currentGeneration: UInt64,
        isCancelled: Bool,
        socketMatches: Bool,
        gatewayConnected: Bool
    ) -> Bool {
        !isCancelled
            && expectedGeneration == currentGeneration
            && socketMatches
            && gatewayConnected
    }

    func publishPluginInstallationStatus(
        expectedSocket: URLSessionWebSocketTask? = nil,
        bootstrapGeneration: UInt64? = nil
    ) async throws {
        if let expectedSocket {
            guard bootstrapGeneration.map({ generation in
                isGatewayConnectedBootstrapCurrent(
                    socket: expectedSocket,
                    generation: generation
                )
            }) ?? (webSocket === expectedSocket) else {
                throw CancellationError()
            }
        }
        let sources = try await pluginSources()
        try Task.checkCancellation()
        if let expectedSocket {
            guard bootstrapGeneration.map({ generation in
                isGatewayConnectedBootstrapCurrent(
                    socket: expectedSocket,
                    generation: generation
                )
            }) ?? (webSocket === expectedSocket) else {
                throw CancellationError()
            }
        }
        if reconcileInstalledPluginIdentities(with: sources.items) {
            try stateStore.save(state)
        }
        try await sendPluginInstallationStatus(expectedSocket: expectedSocket)
    }

    func sendPluginInstallationStatus(
        expectedSocket: URLSessionWebSocketTask? = nil
    ) async throws {
        guard let socket = expectedSocket ?? webSocket,
              webSocket === socket,
              let ownerUserID = state.user?.id,
              let deviceID = state.deviceID else {
            return
        }
        let records = state.installedPluginRecords ?? [:]
        let items = records.values
            .filter { $0.source == nil }
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
        try Task.checkCancellation()
        guard webSocket === socket, gatewayConnected else { return }
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
        cancelGatewayConnectionFailureHandling()
        reconnectTask?.cancel()
        reconnectTask = nil
        await closeGatewayConnection(terminatePluginSessions: true)
    }

    func cancelGatewayConnectionFailureHandling() {
        gatewayConnectionFailureGeneration &+= 1
        gatewayConnectionFailureTask?.cancel()
        gatewayConnectionFailureTask = nil
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
        gatewayConnectedBootstrapGeneration &+= 1
        gatewayConnectedBootstrapTask?.cancel()
        gatewayConnectedBootstrapTask = nil
        let relayTasks = companionRelayTasks.values
        companionRelayTasks.removeAll(keepingCapacity: true)
        relayTasks.forEach { $0.cancel() }
        await companionAgentDrainCoordinator.cancelAll()
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
