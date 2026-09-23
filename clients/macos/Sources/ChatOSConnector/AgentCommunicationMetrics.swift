import Foundation

public enum AgentDocumentCreationMetricOutcome: String, Sendable {
    case succeeded
    case empty
    case tooLarge = "too_large"
    case tooMany = "too_many"
    case runLimitExceeded = "run_limit_exceeded"
    case invalidName = "invalid_name"
    case invalidTitle = "invalid_title"
    case storageFailed = "storage_failed"
}

public enum AgentArtifactUploadMetricOutcome: String, Sendable {
    case succeeded
    case failed
}

public enum AgentDocumentPreviewMetricOutcome: String, Sendable {
    case succeeded
    case failed
}

public enum AgentCommunicationRejectionMetricReason: String, Sendable {
    case messageTooLong = "message_too_long"
    case tooManyDocumentRefs = "too_many_document_refs"
    case duplicateDocumentRef = "duplicate_document_ref"
    case invalidDocumentRef = "invalid_document_ref"
    case documentIntegrityChanged = "document_integrity_changed"
}

public struct AgentCommunicationMetricRow: Sendable, Equatable {
    public let name: String
    public let dimension: String
    public let count: Int64
    public let totalValue: Int64
    public let maximumValue: Int64
    public let updatedAtUnixMs: Int64

    public init(
        name: String,
        dimension: String,
        count: Int64,
        totalValue: Int64,
        maximumValue: Int64,
        updatedAtUnixMs: Int64
    ) {
        self.name = name
        self.dimension = dimension
        self.count = count
        self.totalValue = totalValue
        self.maximumValue = maximumValue
        self.updatedAtUnixMs = updatedAtUnixMs
    }
}
