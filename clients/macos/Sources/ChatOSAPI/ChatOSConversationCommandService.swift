import ChatOSCore
import Foundation

public actor ChatOSConversationCommandService: ConversationCommandServicing {
    private let client: ChatOSAPIClient
    private let attachmentService: any ConversationAttachmentUploading

    public init(
        client: ChatOSAPIClient,
        attachmentService: (any ConversationAttachmentUploading)? = nil
    ) {
        self.client = client
        self.attachmentService = attachmentService ?? ChatOSAttachmentService(client: client)
    }

    public func sendNewTurn(
        _ command: ConversationSendCommand
    ) async throws -> ConversationCommandAck {
        async let settings: RuntimeSettingsDTO = client.request(
            "/conversations/\(command.sessionID.urlPathEncoded)/runtime-settings"
        )
        async let modelConfigs: [ModelConfigDTO] = client.request("/ai-model-configs")

        let (runtime, configs) = try await (settings, modelConfigs)
        let model = try resolveModel(runtime: runtime, configs: configs)
        let attachments = try await attachmentService.upload(
            command.attachments,
            conversationID: command.sessionID
        )
        let request = ChatCommandRequestDTO(
            conversationID: command.sessionID,
            content: command.content,
            attachments: attachments,
            reasoningEnabled: command.reasoningEnabled ?? runtime.reasoningEnabled,
            planMode: command.planModeEnabled ?? runtime.planModeEnabled,
            turnID: command.turnID,
            remoteConnectionID: runtime.remoteConnectionID,
            workspaceRoot: runtime.workspaceRoot,
            modelConfigID: model.id,
            aiModelConfig: .init(
                temperature: model.temperature ?? 0.7,
                modelName: model.modelName,
                thinkingLevel: model.thinkingLevel?.trimmedNonEmptyValue
            )
        )
        let response: ChatCommandResponseDTO = try await client.request(
            "/agent/chat/send",
            method: "POST",
            body: try JSONEncoder().encode(request)
        )
        guard response.accepted != false else {
            throw ChatOSAPIError.server(statusCode: 409, message: "聊天命令未被接受")
        }
        return response.ack(fallbackTurnID: command.turnID)
    }

    public func sendGuidance(
        _ command: ConversationSendCommand
    ) async throws -> ConversationCommandAck {
        let attachments = try await attachmentService.upload(
            command.attachments,
            conversationID: command.sessionID
        )
        let request = GuidanceRequestDTO(
            conversationID: command.sessionID,
            turnID: command.turnID,
            content: command.content,
            attachments: attachments
        )
        let response: ChatCommandResponseDTO
        do {
            response = try await client.request(
                "/agent/chat/guidance",
                method: "POST",
                body: try JSONEncoder().encode(request)
            )
        } catch let error as ChatOSAPIError where error.isInactiveGuidanceConflict {
            throw ConversationCommandError.guidanceTargetInactive
        }
        guard response.accepted != false else {
            throw ChatOSAPIError.server(statusCode: 409, message: "追加指令未被接受")
        }
        return response.ack(fallbackTurnID: command.turnID)
    }

    public func stopTurn(conversationID: String, turnID: String?) async throws {
        let response: StopChatResponseDTO = try await client.request(
            "/agent/chat/stop",
            method: "POST",
            body: try JSONEncoder().encode(
                StopChatRequestDTO(
                    conversationID: conversationID,
                    turnID: turnID?.trimmedNonEmptyValue
                )
            )
        )
        guard response.success else {
            throw ChatOSAPIError.server(
                statusCode: 409,
                message: response.message?.trimmedNonEmptyValue ?? "当前 AI 执行无法停止"
            )
        }
    }

    private func resolveModel(
        runtime: RuntimeSettingsDTO,
        configs: [ModelConfigDTO]
    ) throws -> ModelConfigDTO {
        let enabled = configs.filter {
            $0.enabled != false && $0.modelName.trimmedNonEmptyValue != nil
        }
        if let selectedID = runtime.selectedModelID?.trimmedNonEmptyValue,
           let selected = enabled.first(where: { $0.id == selectedID }) {
            return selected
        }
        guard let fallback = enabled.first else {
            throw ChatOSAPIError.missingModelConfiguration
        }
        return fallback
    }
}

private extension ChatOSAPIError {
    var isInactiveGuidanceConflict: Bool {
        let statusCode: Int
        let message: String
        switch self {
        case let .server(code, value):
            statusCode = code
            message = value
        case let .serverDetail(code, value, _, _):
            statusCode = code
            message = value
        default:
            return false
        }
        guard statusCode == 409 else { return false }
        let normalized = message.lowercased()
        return normalized.contains("目标轮次已结束")
            || normalized.contains("无法追加指令")
            || normalized.contains("turn has ended")
            || normalized.contains("turn is no longer active")
    }
}
