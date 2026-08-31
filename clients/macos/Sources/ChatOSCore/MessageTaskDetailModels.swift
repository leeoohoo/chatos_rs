import Foundation

public struct MessageTaskReference: Identifiable, Sendable, Equatable {
    public let id: String
    public var title: String?
    public var status: String?

    public init(id: String, title: String? = nil, status: String? = nil) {
        self.id = id
        self.title = title
        self.status = status
    }
}

public struct MessageTaskModelConfigSummary: Sendable, Equatable {
    public var id: String
    public var name: String?
    public var provider: String?
    public var model: String?

    public init(
        id: String,
        name: String? = nil,
        provider: String? = nil,
        model: String? = nil
    ) {
        self.id = id
        self.name = name
        self.provider = provider
        self.model = model
    }

    public var displayName: String {
        let providerModel = [provider, model]
            .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
            .joined(separator: "/")
        return [name, providerModel.isEmpty ? nil : providerModel]
            .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
            .joined(separator: " · ")
    }
}

public struct MessageTaskLastRunSummary: Sendable, Equatable {
    public var id: String
    public var status: String?
    public var modelPhaseStatus: String?
    public var resultSummary: String?
    public var reportContent: String?
    public var errorMessage: String?
    public var startedAt: Date?
    public var finishedAt: Date?

    public init(
        id: String,
        status: String? = nil,
        modelPhaseStatus: String? = nil,
        resultSummary: String? = nil,
        reportContent: String? = nil,
        errorMessage: String? = nil,
        startedAt: Date? = nil,
        finishedAt: Date? = nil
    ) {
        self.id = id
        self.status = status
        self.modelPhaseStatus = modelPhaseStatus
        self.resultSummary = resultSummary
        self.reportContent = reportContent
        self.errorMessage = errorMessage
        self.startedAt = startedAt
        self.finishedAt = finishedAt
    }
}

public extension MessageTask {
    func merging(run: MessageTaskRun) -> MessageTask {
        var copy = self
        copy.lastRunID = run.id
        copy.lastRunStatus = run.status
        copy.lastRun = MessageTaskLastRunSummary(
            id: run.id,
            status: run.status,
            modelPhaseStatus: run.modelPhaseStatus,
            resultSummary: run.resultSummary,
            reportContent: run.reportContent,
            errorMessage: run.errorMessage,
            startedAt: run.startedAt,
            finishedAt: run.finishedAt
        )
        return copy
    }
}
