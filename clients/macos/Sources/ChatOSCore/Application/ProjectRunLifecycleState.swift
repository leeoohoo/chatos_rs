import Foundation

/// Request identity for project runtime presentation writes.
public struct ProjectRunRequest: Sendable, Equatable {
    public let id: UInt64
    public let projectID: String

    public init(id: UInt64, projectID: String) {
        self.id = id
        self.projectID = projectID
    }
}

/// Coordinates snapshot loads, realtime state refreshes, and exclusive
/// mutations. The state is UI-framework independent so macOS and React can
/// preserve the same latest-wins and mutation ownership rules.
public struct ProjectRunLifecycleState: Sendable, Equatable {
    private var sequence: UInt64 = 0
    private var activeLoad: ProjectRunRequest?
    private var activeStateRefresh: ProjectRunRequest?
    private var activeMutation: ProjectRunRequest?

    public init() {}

    public var isLoading: Bool { activeLoad != nil }
    public var isMutating: Bool { activeMutation != nil }

    public mutating func beginLoad(projectID: String) -> ProjectRunRequest? {
        guard activeMutation == nil else { return nil }
        let request = next(projectID: projectID)
        activeLoad = request
        activeStateRefresh = request
        return request
    }

    public func acceptsLoad(_ request: ProjectRunRequest) -> Bool {
        activeLoad == request && activeMutation == nil
    }

    public func acceptsState(_ request: ProjectRunRequest) -> Bool {
        activeStateRefresh == request && activeMutation == nil
    }

    public mutating func finishLoad(_ request: ProjectRunRequest) {
        if activeLoad == request { activeLoad = nil }
        if activeStateRefresh == request { activeStateRefresh = nil }
    }

    public mutating func beginStateRefresh(projectID: String) -> ProjectRunRequest? {
        guard activeMutation == nil else { return nil }
        let request = next(projectID: projectID)
        activeStateRefresh = request
        return request
    }

    public mutating func finishStateRefresh(_ request: ProjectRunRequest) {
        if activeStateRefresh == request { activeStateRefresh = nil }
    }

    public mutating func beginMutation(projectID: String) -> ProjectRunRequest? {
        guard activeMutation == nil else { return nil }
        let request = next(projectID: projectID)
        activeMutation = request
        activeLoad = nil
        activeStateRefresh = nil
        return request
    }

    public func acceptsMutation(_ request: ProjectRunRequest) -> Bool {
        activeMutation == request
    }

    public mutating func finishMutation(_ request: ProjectRunRequest) {
        if activeMutation == request { activeMutation = nil }
    }

    public mutating func reset() {
        sequence &+= 1
        activeLoad = nil
        activeStateRefresh = nil
        activeMutation = nil
    }

    private mutating func next(projectID: String) -> ProjectRunRequest {
        sequence &+= 1
        return .init(id: sequence, projectID: projectID)
    }
}
