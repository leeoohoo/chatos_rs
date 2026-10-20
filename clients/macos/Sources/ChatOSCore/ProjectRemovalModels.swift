import Foundation

public protocol ProjectRemovalServicing: Sendable {
    func projectRecord(ownerUserID: String, projectID: String) async throws -> LocalProjectRecord?
    func removeProject(
        ownerUserID: String,
        projectID: String,
        expectedRevision: Int64
    ) async throws
}

public struct ProjectRemovalRequest: Sendable, Equatable {
    public let ownerUserID: String
    public let accountGeneration: UInt64
    public let projectID: String

    public init(ownerUserID: String, accountGeneration: UInt64, projectID: String) {
        self.ownerUserID = ownerUserID
        self.accountGeneration = accountGeneration
        self.projectID = projectID
    }
}

public struct ProjectRemovalReceipt: Sendable, Equatable {
    public let request: ProjectRemovalRequest
    public let removedRevision: Int64

    public init(request: ProjectRemovalRequest, removedRevision: Int64) {
        self.request = request
        self.removedRevision = removedRevision
    }
}
