import ChatOSCore
import Foundation

public struct ChatOSProjectExecutionService: ProjectExecutionServicing {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func fetchExecution(
        _ identity: ProjectExecutionIdentity
    ) async throws -> ProjectRequirementExecutionLaunch? {
        var components = URLComponents()
        components.percentEncodedPath = "/projects/\(identity.projectID.urlPathEncoded)/requirements/\(identity.requirementID.urlPathEncoded)/execution-plan"
        components.queryItems = [
            URLQueryItem(name: "conversation_id", value: identity.conversationID),
            URLQueryItem(name: "execution_group_id", value: identity.executionGroupID),
        ]
        let response: ProjectRequirementExecutionDTO = try await client.request(
            components.string ?? components.path
        )
        guard response.found != false,
              let conversationID = response.conversationID?.trimmedNonEmptyValue,
              let executionGroupID = response.executionGroupID?.trimmedNonEmptyValue else {
            return nil
        }
        return response.domainModel(
            fallbackProjectID: identity.projectID,
            fallbackRequirementID: identity.requirementID,
            conversationID: conversationID,
            executionGroupID: executionGroupID
        )
    }

    public func confirmExecution(
        _ identity: ProjectExecutionIdentity
    ) async throws -> ProjectExecutionActionResult {
        try await mutate(identity, action: "confirm-execution", discardTasks: nil)
    }

    public func abandonPlan(
        _ identity: ProjectExecutionIdentity
    ) async throws -> ProjectExecutionActionResult {
        try await mutate(identity, action: "stop", discardTasks: true)
    }

    private func mutate(
        _ identity: ProjectExecutionIdentity,
        action: String,
        discardTasks: Bool?
    ) async throws -> ProjectExecutionActionResult {
        let response: ProjectExecutionActionResponseDTO = try await client.request(
            "/projects/\(identity.projectID.urlPathEncoded)/requirements/\(identity.requirementID.urlPathEncoded)/\(action)",
            method: "POST",
            body: try JSONEncoder().encode(
                ProjectExecutionActionRequestDTO(
                    executionGroupID: identity.executionGroupID,
                    conversationID: identity.conversationID,
                    contactID: identity.contactID,
                    discardTasks: discardTasks
                )
            )
        )
        return response.model
    }
}
