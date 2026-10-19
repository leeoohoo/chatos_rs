import Foundation

/// Application use case for the macOS project-to-conversation preparation flow.
///
/// The context loader and conversation preparer are ports. Connector-specific project lookup,
/// resource binding, and Host IPC remain in their adapters. Concurrent requests for the same
/// owner/project share one operation so callers cannot race duplicate Conversation creation.
@MainActor
public final class PrepareProjectConversationUseCase {
    private struct Key: Hashable {
        let ownerUserID: String
        let projectID: String
    }

    private struct InFlight {
        let id: UUID
        let task: Task<String, Error>
    }

    private let contextLoader: any ProjectContextLoading
    private let projectConversationPort: any ProjectConversationPort
    private var inFlight: [Key: InFlight] = [:]

    public init(
        contextLoader: any ProjectContextLoading,
        projectConversationPort: any ProjectConversationPort
    ) {
        self.contextLoader = contextLoader
        self.projectConversationPort = projectConversationPort
    }

    public func execute(
        ownerUserID: String,
        project: WorkspaceProject,
        contact: WorkspaceContact
    ) async throws -> String {
        let key = Key(ownerUserID: ownerUserID, projectID: project.id)
        if let existing = inFlight[key] {
            return try await existing.task.value
        }

        let contextLoader = contextLoader
        let projectConversationPort = projectConversationPort
        let task = Task {
            var preparedProject = project
            preparedProject.projectContext = try await contextLoader.projectContext(
                ownerUserID: ownerUserID,
                projectID: project.id
            )
            try Task.checkCancellation()
            return try await projectConversationPort.ensureConversation(
                project: preparedProject,
                contact: contact
            )
        }
        let id = UUID()
        inFlight[key] = InFlight(id: id, task: task)
        defer {
            if inFlight[key]?.id == id {
                inFlight.removeValue(forKey: key)
            }
        }
        return try await task.value
    }

    public func cancel(ownerUserID: String, projectID: String) {
        let key = Key(ownerUserID: ownerUserID, projectID: projectID)
        inFlight.removeValue(forKey: key)?.task.cancel()
    }

    public func cancelAll() {
        let tasks = inFlight.values.map(\.task)
        inFlight.removeAll(keepingCapacity: false)
        tasks.forEach { $0.cancel() }
    }
}
