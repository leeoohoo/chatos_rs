import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    nonisolated static func isCompanionRelayMessageType(_ messageType: String) -> Bool {
        switch messageType {
        case "companion_resources_request",
             "companion_resolve_resource_request",
             "companion_approvals_request",
             "companion_resolve_approval_request":
            true
        default:
            false
        }
    }

    func handleCompanionRelayMessage(
        _ data: Data,
        socket: URLSessionWebSocketTask
    ) async {
        let decoded = try? JSONDecoder().decode(NativeRelayRequest.self, from: data)
        let requestID = decoded?.requestID ?? ""
        let responseType = Self.companionResponseType(for: decoded?.type)
        do {
            guard let request = decoded else { throw NativeCompanionRelayError.unsupportedRequest }
            let response = try await processCompanionRelay(request)
            try await sendRelayResponse(response, socket: socket)
        } catch {
            let status = (error as? NativeCompanionRelayError)?.status ?? 500
            let response = NativeRelayResponse(
                type: responseType,
                requestID: requestID,
                status: status,
                body: .object(["error": .string(error.localizedDescription)])
            )
            try? await sendRelayResponse(response, socket: socket)
        }
    }

    private func processCompanionRelay(
        _ request: NativeRelayRequest
    ) async throws -> NativeRelayResponse {
        guard let ownerUserID = state.user?.id,
              let deviceID = state.deviceID else {
            throw NativeCompanionRelayError.invalidContext
        }
        let runtimeConfig = try await managedRuntimeConfig()
        try NativeRelayVerifier().verify(
            request,
            trust: runtimeConfig.remoteControlTrust,
            ownerUserID: ownerUserID,
            deviceID: deviceID,
            seenNonces: &seenRelayNonces
        )
        let body: NativeJSONValue
        switch request.type {
        case "companion_resources_request":
            guard let companionRuntime else {
                throw NativeCompanionRelayError.runtimeUnavailable
            }
            let resources = await companionRuntime.companionResources()
            body = try Self.nativeJSON(resources)
        case "companion_resolve_resource_request":
            guard let companionRuntime else {
                throw NativeCompanionRelayError.runtimeUnavailable
            }
            let payload = try request.body.decode(CompanionResolveRequest.self)
            let id = payload.resourceID.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !id.isEmpty else { throw NativeCompanionRelayError.missingResourceID }
            let resource = try await companionRuntime.resolveCompanionResource(id: id)
            body = try Self.nativeJSON(resource)
        case "companion_approvals_request":
            let approvals = try await fetchPendingApprovals().map(Self.companionApproval)
            body = try Self.nativeJSON(approvals)
        case "companion_resolve_approval_request":
            let payload = try request.body.decode(CompanionApprovalDecisionRequest.self)
            let id = payload.approvalID.trimmingCharacters(in: .whitespacesAndNewlines)
            let decision = payload.decision.trimmingCharacters(in: .whitespacesAndNewlines)
            _ = try Self.validateCompanionApprovalResolution(
                id: id,
                decision: decision,
                pending: try await fetchPendingApprovals()
            )
            try await resolveApproval(id: id, decision: decision)
            body = .object(["success": .bool(true), "approval_id": .string(id)])
        default:
            throw NativeCompanionRelayError.unsupportedRequest
        }
        return NativeRelayResponse(
            type: Self.companionResponseType(for: request.type),
            requestID: request.requestID,
            status: 200,
            body: body
        )
    }

    nonisolated private static func nativeJSON<T: Encodable>(_ value: T) throws -> NativeJSONValue {
        try JSONDecoder().decode(NativeJSONValue.self, from: JSONEncoder().encode(value))
    }

    nonisolated static func companionApproval(
        _ approval: LocalConnectorPendingApproval
    ) -> LocalConnectorCompanionApproval {
        let context = URL(fileURLWithPath: approval.cwd).lastPathComponent
        return LocalConnectorCompanionApproval(
            id: approval.id,
            command: approval.command,
            context: context.isEmpty ? nil : context,
            source: approval.source,
            risk: approval.risk,
            reason: approval.reason,
            createdAt: approval.createdAt,
            availableDecisions: approval.availableDecisions.filter {
                Self.isCompanionApprovalDecision($0)
            }
        )
    }

    nonisolated static func isCompanionApprovalDecision(_ decision: String) -> Bool {
        ["accept", "acceptForSession", "decline"].contains(decision)
    }

    nonisolated static func validateCompanionApprovalResolution(
        id: String,
        decision: String,
        pending: [LocalConnectorPendingApproval]
    ) throws -> LocalConnectorPendingApproval {
        guard !id.isEmpty else { throw NativeCompanionRelayError.missingApprovalID }
        guard Self.isCompanionApprovalDecision(decision) else {
            throw NativeCompanionRelayError.invalidApprovalDecision
        }
        guard let approval = pending.first(where: { $0.id == id }) else {
            throw NativeCompanionRelayError.approvalNotFound
        }
        guard approval.availableDecisions.contains(decision) else {
            throw NativeCompanionRelayError.invalidApprovalDecision
        }
        return approval
    }

    nonisolated private static func companionResponseType(for requestType: String?) -> String {
        switch requestType {
        case "companion_resources_request": "companion_resources_response"
        case "companion_resolve_resource_request": "companion_resolve_resource_response"
        case "companion_approvals_request": "companion_approvals_response"
        case "companion_resolve_approval_request": "companion_resolve_approval_response"
        default: "companion_error_response"
        }
    }
}

private struct CompanionResolveRequest: Decodable {
    var resourceID: String

    enum CodingKeys: String, CodingKey {
        case resourceID = "resource_id"
    }
}

private struct CompanionApprovalDecisionRequest: Decodable {
    var approvalID: String
    var decision: String

    enum CodingKeys: String, CodingKey {
        case approvalID = "approval_id"
        case decision
    }
}

enum NativeCompanionRelayError: LocalizedError {
    case invalidContext
    case runtimeUnavailable
    case missingResourceID
    case missingApprovalID
    case invalidApprovalDecision
    case approvalNotFound
    case unsupportedRequest

    var status: Int {
        switch self {
        case .runtimeUnavailable: 503
        case .invalidContext: 401
        case .approvalNotFound: 404
        case .missingResourceID, .missingApprovalID, .invalidApprovalDecision, .unsupportedRequest: 400
        }
    }

    var errorDescription: String? {
        switch self {
        case .invalidContext: "本机 Companion 上下文无效。"
        case .runtimeUnavailable: "桌面客户端会话目录尚未就绪。"
        case .missingResourceID: "resource_id 不能为空。"
        case .missingApprovalID: "approval_id 不能为空。"
        case .invalidApprovalDecision: "审批决定无效。"
        case .approvalNotFound: "这个审批请求已经处理或不存在。"
        case .unsupportedRequest: "不支持的 Companion 请求。"
        }
    }
}
