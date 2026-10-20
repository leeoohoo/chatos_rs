import Foundation

/// Removes only the owner-scoped registry record. Presentation cleanup remains
/// client-owned and is applied only when the returned account context is still
/// current. The project directory itself is never touched.
public struct ProjectRemovalUseCase: Sendable {
    private let service: any ProjectRemovalServicing

    public init(service: any ProjectRemovalServicing) {
        self.service = service
    }

    public func remove(_ request: ProjectRemovalRequest) async throws -> ProjectRemovalReceipt {
        try ProjectRegistryValidation.identifier(request.ownerUserID, field: "ownerUserID")
        try ProjectRegistryValidation.identifier(request.projectID, field: "projectID")
        guard let record = try await service.projectRecord(
            ownerUserID: request.ownerUserID,
            projectID: request.projectID
        ) else {
            throw ProjectRegistryError.notFound
        }
        try Task.checkCancellation()
        try await service.removeProject(
            ownerUserID: request.ownerUserID,
            projectID: request.projectID,
            expectedRevision: record.revision
        )
        return .init(request: request, removedRevision: record.revision)
    }
}
