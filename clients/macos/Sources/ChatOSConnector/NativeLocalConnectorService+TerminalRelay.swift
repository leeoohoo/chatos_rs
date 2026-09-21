import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    func handleTerminalRelayMessage(
        _ data: Data,
        socket: URLSessionWebSocketTask
    ) async {
        let decoded = try? JSONDecoder().decode(NativeRelayRequest.self, from: data)
        let requestID = decoded?.requestID ?? ""
        let responseType = Self.terminalRelayResponseType(for: decoded?.type)
        do {
            let request = try decoded ?? JSONDecoder().decode(NativeRelayRequest.self, from: data)
            try await verifyTerminalRelayRequest(request)
            switch request.type {
            case "terminal_exec_request":
                let response = try await processTerminalRelay(request, verified: true)
                try await sendRelayResponse(response, socket: socket)
            case "terminal_session_create_request":
                let response = try await createLocalTerminalRelaySession(request)
                try await sendRelayResponse(response, socket: socket)
            case "remote_terminal_session_create_request":
                let response = try await createRemoteTerminalRelaySession(request)
                try await sendRelayResponse(response, socket: socket)
            case "terminal_input", "terminal_command", "terminal_resize",
                 "terminal_snapshot_request", "terminal_close",
                 "remote_terminal_input", "remote_terminal_resize",
                 "remote_terminal_snapshot_request", "remote_terminal_close":
                try await processTerminalRelayControl(request)
            default:
                throw NativeTerminalRelayError.unsupportedRequest
            }
        } catch let challenge as RemoteVerificationChallenge {
            let response = NativeRelayResponse(
                type: responseType,
                requestID: requestID,
                status: 409,
                body: .object([
                    "code": .string("second_factor_required"),
                    "error": .string(challenge.prompt),
                    "prompt": .string(challenge.prompt),
                ])
            )
            try? await sendRelayResponse(response, socket: socket)
        } catch {
            if let request = decoded,
               !Self.terminalRelayExpectsResponse(request.type),
               case let .object(body) = request.body,
               case let .string(sessionID)? = body["terminal_session_id"] {
                queueTerminalRelayEvent(.init(
                    type: "terminal_error",
                    terminalSessionID: sessionID,
                    body: .object([
                        "error": .string(error.localizedDescription),
                        "recoverable": .bool(true),
                    ])
                ))
                return
            }
            let response = NativeRelayResponse(
                type: responseType,
                requestID: requestID,
                status: 400,
                body: .object(["error": .string(error.localizedDescription)])
            )
            try? await sendRelayResponse(response, socket: socket)
        }
    }

    private static func terminalRelayResponseType(for requestType: String?) -> String {
        switch requestType {
        case "terminal_session_create_request", "remote_terminal_session_create_request":
            "terminal_session_create_response"
        default:
            "terminal_response"
        }
    }

    private static func terminalRelayExpectsResponse(_ requestType: String) -> Bool {
        requestType == "terminal_exec_request"
            || requestType == "terminal_session_create_request"
            || requestType == "remote_terminal_session_create_request"
    }

    private func verifyTerminalRelayRequest(_ request: NativeRelayRequest) async throws {
        guard let ownerUserID = state.user?.id,
              let deviceID = state.deviceID,
              state.workspaces.contains(where: { $0.id == request.workspaceID }) else {
            throw NativeTerminalRelayError.invalidContext
        }
        let runtime = try await managedRuntimeConfig()
        try NativeRelayVerifier().verify(
            request,
            trust: runtime.remoteControlTrust,
            ownerUserID: ownerUserID,
            deviceID: deviceID,
            seenNonces: &seenRelayNonces
        )
    }

    private func processTerminalRelay(
        _ request: NativeRelayRequest,
        verified: Bool = false
    ) async throws -> NativeRelayResponse {
        guard request.type == "terminal_exec_request" else {
            throw NativeTerminalRelayError.unsupportedRequest
        }
        guard let workspace = state.workspaces.first(where: { $0.id == request.workspaceID }) else {
            throw NativeTerminalRelayError.invalidContext
        }
        if !verified { try await verifyTerminalRelayRequest(request) }

        let body = try request.body.decode(NativeTerminalRelayBody.self)
        let command = body.command.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !command.isEmpty else { throw NativeTerminalRelayError.emptyCommand }
        let projectRoot = try resolveDirectory(
            request.header("x-local-connector-project-root")
                ?? request.header("x-local-connector-cwd")
                ?? ".",
            relativeTo: URL(fileURLWithPath: workspace.absoluteRoot),
            workspace: workspace
        )
        let cwd = try resolveDirectory(
            body.cwd ?? ".",
            relativeTo: projectRoot,
            workspace: workspace
        )
        let risk = NativeApprovalRiskEvaluator.evaluate(command: command, arguments: body.args)
        let approval = await approvalDecision(
            requestID: request.requestID,
            command: command,
            arguments: body.args,
            cwd: cwd,
            projectRoot: projectRoot,
            source: body.source ?? "terminal-relay",
            risk: risk,
            workspaceID: request.workspaceID
        )
        switch approval {
        case let .deny(reason), let .askUser(reason):
            appendApprovalHistory(
                command: command,
                arguments: body.args,
                cwd: cwd.path,
                source: body.source ?? "terminal-relay",
                decision: "denied",
                risk: risk,
                reason: reason
            )
            return terminalResponse(
                requestID: request.requestID,
                command: command,
                arguments: body.args,
                cwd: cwd.path,
                result: nil,
                error: reason,
                approvalDecision: "denied"
            )
        case let .approve(reason, _):
            appendApprovalHistory(
                command: command,
                arguments: body.args,
                cwd: cwd.path,
                source: body.source ?? "terminal-relay",
                decision: "approved",
                risk: risk,
                reason: reason
            )
        }

        let result = try await Task.detached {
            try NativeTerminalExecutor.execute(
                command: command,
                args: body.args,
                cwd: cwd.path,
                workspace: workspace
            )
        }.value
        appendCommandHistory(
            result: result,
            display: ([command] + body.args).joined(separator: " "),
            workspace: workspace,
            source: body.source ?? "terminal-relay"
        )
        return terminalResponse(
            requestID: request.requestID,
            command: command,
            arguments: body.args,
            cwd: cwd.path,
            result: result,
            error: result.error,
            approvalDecision: "approved"
        )
    }

    private func createLocalTerminalRelaySession(
        _ request: NativeRelayRequest
    ) async throws -> NativeRelayResponse {
        guard let workspace = state.workspaces.first(where: { $0.id == request.workspaceID }) else {
            throw NativeTerminalRelayError.invalidContext
        }
        let body = try request.body.decode(NativeTerminalSessionCreateRelayBody.self)
        let sessionID = try validatedTerminalSessionID(body.terminalSessionID)
        let base = try resolveDirectory(
            request.header("x-local-connector-cwd") ?? ".",
            relativeTo: URL(fileURLWithPath: workspace.absoluteRoot),
            workspace: workspace
        )
        let cwd = try resolveDirectory(body.cwd ?? ".", relativeTo: base, workspace: workspace)
        let columns = normalizedTerminalDimension(body.columns ?? 80)
        let rows = normalizedTerminalDimension(body.rows ?? 24)

        let session: NativeLocalTerminalRelaySession
        if let existing = terminalRelaySessions[sessionID] {
            guard let local = existing as? NativeLocalTerminalRelaySession,
                  local.workspaceID == workspace.id,
                  local.workingDirectory == cwd.path else {
                throw NativeTerminalRelayError.sessionIdentityConflict
            }
            if local.running {
                session = local
                session.resize(columns: columns, rows: rows)
            } else {
                terminalRelaySessions.removeValue(forKey: sessionID)?.close()
                session = try makeLocalTerminalRelaySession(
                    sessionID: sessionID,
                    workspaceID: workspace.id,
                    cwd: cwd.path,
                    columns: columns,
                    rows: rows
                )
            }
        } else {
            session = try makeLocalTerminalRelaySession(
                sessionID: sessionID,
                workspaceID: workspace.id,
                cwd: cwd.path,
                columns: columns,
                rows: rows
            )
        }
        attachTerminalRelaySession(session)
        return terminalSessionCreateResponse(
            requestID: request.requestID,
            sessionID: sessionID,
            snapshot: session.snapshot(maximumLines: 500)
        )
    }

    private func makeLocalTerminalRelaySession(
        sessionID: String,
        workspaceID: String,
        cwd: String,
        columns: Int,
        rows: Int
    ) throws -> NativeLocalTerminalRelaySession {
        try enforceTerminalSessionCapacity(remote: false)
        let session = NativeLocalTerminalRelaySession(
            terminalSessionID: sessionID,
            workspaceID: workspaceID,
            workingDirectory: cwd,
            columns: columns,
            rows: rows
        )
        attachTerminalRelaySession(session)
        do {
            try session.start()
            terminalRelaySessions[sessionID] = session
            return session
        } catch {
            session.close()
            throw error
        }
    }

    private func createRemoteTerminalRelaySession(
        _ request: NativeRelayRequest
    ) async throws -> NativeRelayResponse {
        let body = try request.body.decode(NativeRemoteTerminalSessionCreateRelayBody.self)
        let sessionID = try validatedTerminalSessionID(body.terminalSessionID)
        guard case let .object(connection) = body.connection,
              case let .string(rawConnectionID)? = connection["id"] else {
            throw NativeTerminalRelayError.missingRemoteConnection
        }
        let connectionID = rawConnectionID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !connectionID.isEmpty,
              let runtime = remoteConnectionRuntime,
              let provider = runtime as? any NativeRemoteTerminalSessionProviding else {
            throw NativeTerminalRelayError.missingRemoteConnection
        }
        let columns = normalizedTerminalDimension(body.columns ?? 80)
        let rows = normalizedTerminalDimension(body.rows ?? 24)

        let session: NativeRemoteTerminalRelaySession
        if let existing = terminalRelaySessions[sessionID] {
            guard let remote = existing as? NativeRemoteTerminalRelaySession,
                  remote.workspaceID == request.workspaceID,
                  remote.connectionID == connectionID else {
                throw NativeTerminalRelayError.sessionIdentityConflict
            }
            if remote.running {
                session = remote
                session.resize(columns: columns, rows: rows)
            } else {
                terminalRelaySessions.removeValue(forKey: sessionID)?.close()
                session = try await makeRemoteTerminalRelaySession(
                    sessionID: sessionID,
                    workspaceID: request.workspaceID,
                    connectionID: connectionID,
                    provider: provider,
                    verificationCode: body.verificationCode,
                    columns: columns,
                    rows: rows
                )
            }
        } else {
            session = try await makeRemoteTerminalRelaySession(
                sessionID: sessionID,
                workspaceID: request.workspaceID,
                connectionID: connectionID,
                provider: provider,
                verificationCode: body.verificationCode,
                columns: columns,
                rows: rows
            )
        }
        attachTerminalRelaySession(session)
        return terminalSessionCreateResponse(
            requestID: request.requestID,
            sessionID: sessionID,
            snapshot: session.snapshot(maximumLines: 500)
        )
    }

    private func makeRemoteTerminalRelaySession(
        sessionID: String,
        workspaceID: String,
        connectionID: String,
        provider: any NativeRemoteTerminalSessionProviding,
        verificationCode: String?,
        columns: Int,
        rows: Int
    ) async throws -> NativeRemoteTerminalRelaySession {
        try enforceTerminalSessionCapacity(remote: true)
        let transport = try await provider.makeRemoteTerminalSession(
            connectionID: connectionID,
            verificationCode: verificationCode
        )
        let session = NativeRemoteTerminalRelaySession(
            terminalSessionID: sessionID,
            workspaceID: workspaceID,
            connectionID: connectionID,
            session: transport
        )
        attachTerminalRelaySession(session)
        do {
            try session.start(columns: columns, rows: rows)
            terminalRelaySessions[sessionID] = session
            return session
        } catch {
            session.close()
            throw error
        }
    }

    private func processTerminalRelayControl(_ request: NativeRelayRequest) async throws {
        let body = try request.body.decode(NativeTerminalControlRelayBody.self)
        let sessionID = try validatedTerminalSessionID(body.terminalSessionID)
        guard let session = terminalRelaySessions[sessionID],
              session.workspaceID == request.workspaceID else {
            throw NativeTerminalRelayError.sessionNotFound
        }
        let remoteControl = request.type.hasPrefix("remote_terminal_")
        guard remoteControl == (session is NativeRemoteTerminalRelaySession) else {
            throw NativeTerminalRelayError.sessionIdentityConflict
        }

        switch request.type {
        case "terminal_input", "remote_terminal_input":
            try session.send(Data((body.data ?? "").utf8))
        case "terminal_command":
            // Explicit command metadata is an audit hint, never inferred from
            // raw keystrokes and never written into the PTY a second time.
            break
        case "terminal_resize", "remote_terminal_resize":
            session.resize(
                columns: normalizedTerminalDimension(body.columns ?? 80),
                rows: normalizedTerminalDimension(body.rows ?? 24)
            )
        case "terminal_snapshot_request", "remote_terminal_snapshot_request":
            publishTerminalRelaySnapshot(
                sessionID: sessionID,
                snapshot: session.snapshot(maximumLines: body.lines ?? 500)
            )
        case "terminal_close", "remote_terminal_close":
            terminalRelaySessions.removeValue(forKey: sessionID)?.close()
        default:
            throw NativeTerminalRelayError.unsupportedRequest
        }
    }

    private func terminalSessionCreateResponse(
        requestID: String,
        sessionID: String,
        snapshot: NativeTerminalRelaySnapshot
    ) -> NativeRelayResponse {
        .init(
            type: "terminal_session_create_response",
            requestID: requestID,
            status: 200,
            body: .object([
                "terminal_session_id": .string(sessionID),
                "snapshot": .string(snapshot.data),
                "base_sequence": .number(Double(snapshot.baseSequence)),
                "sequence": .number(Double(snapshot.sequence)),
                "truncated": .bool(snapshot.truncated),
                "protocol_version": .number(2),
                "busy": .bool(false),
            ])
        )
    }

    private func attachTerminalRelaySession(_ session: any NativeTerminalRelaySessionProtocol) {
        ensureTerminalRelayEventPump()
        let pump = terminalRelayEventPump
        session.attach { event in pump.yield(event) }
    }

    private func ensureTerminalRelayEventPump() {
        guard terminalRelayEventTask == nil else { return }
        let events = terminalRelayEventPump.events
        terminalRelayEventTask = Task { [weak self] in
            for await event in events {
                guard let self else { return }
                await self.publishTerminalRelayEvent(event)
            }
        }
    }

    private func queueTerminalRelayEvent(_ event: NativeTerminalRelayEvent) {
        ensureTerminalRelayEventPump()
        terminalRelayEventPump.yield(event)
    }

    private func publishTerminalRelaySnapshot(
        sessionID: String,
        snapshot: NativeTerminalRelaySnapshot
    ) {
        queueTerminalRelayEvent(.init(
            type: "terminal_snapshot",
            terminalSessionID: sessionID,
            body: .object([
                "data": .string(snapshot.data),
                "base_sequence": .number(Double(snapshot.baseSequence)),
                "sequence": .number(Double(snapshot.sequence)),
                "truncated": .bool(snapshot.truncated),
                "protocol_version": .number(2),
            ])
        ))
    }

    private func publishTerminalRelayEvent(_ event: NativeTerminalRelayEvent) async {
        guard let socket = webSocket else { return }
        let envelope = NativeTerminalRelayEventEnvelope(
            type: event.type,
            terminalSessionID: event.terminalSessionID,
            body: event.body
        )
        guard let text = try? String(
            data: JSONEncoder().encode(envelope),
            encoding: .utf8
        ) else { return }
        try? await socket.send(.string(text))
    }

    func closeAllTerminalRelaySessions() {
        let sessions = terminalRelaySessions.values
        terminalRelaySessions.removeAll()
        sessions.forEach { $0.close() }
    }

    private func enforceTerminalSessionCapacity(remote: Bool) throws {
        let kindCount = terminalRelaySessions.values.reduce(into: 0) { count, session in
            if (session is NativeRemoteTerminalRelaySession) == remote { count += 1 }
        }
        guard kindCount < 16 else {
            throw NativeTerminalRelayError.sessionCapacityExceeded
        }
    }

    private func validatedTerminalSessionID(_ value: String) throws -> String {
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalized.isEmpty, normalized.utf8.count <= 256 else {
            throw NativeTerminalRelayError.invalidSessionID
        }
        return normalized
    }

    private func normalizedTerminalDimension(_ value: Int) -> Int {
        min(max(value, 1), 1_000)
    }
}

private struct NativeTerminalRelayEventEnvelope: Encodable {
    var type: String
    var terminalSessionID: String
    var body: NativeJSONValue

    enum CodingKeys: String, CodingKey {
        case type, body
        case terminalSessionID = "terminal_session_id"
    }
}

enum NativeTerminalRelayError: LocalizedError {
    case unsupportedRequest
    case invalidContext
    case emptyCommand
    case unsafeDirectory
    case invalidResponse
    case invalidSessionID
    case sessionNotFound
    case sessionIdentityConflict
    case sessionCapacityExceeded
    case missingRemoteConnection

    var errorDescription: String? {
        switch self {
        case .unsupportedRequest: "不支持的 Relay 请求"
        case .invalidContext: "Relay 请求与当前设备或工作区不匹配"
        case .emptyCommand: "终端请求缺少命令"
        case .unsafeDirectory: "终端请求目录无效或超出授权工作区"
        case .invalidResponse: "无法编码终端 Relay 响应"
        case .invalidSessionID: "终端会话 ID 无效"
        case .sessionNotFound: "当前工作区中找不到终端会话"
        case .sessionIdentityConflict: "终端会话 ID 已绑定到其他工作区、目录或连接"
        case .sessionCapacityExceeded: "本机同类终端会话已达到 16 个上限"
        case .missingRemoteConnection: "远程终端缺少可用的本机连接配置"
        }
    }
}
