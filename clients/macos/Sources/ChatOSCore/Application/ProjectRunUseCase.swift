import Foundation

public struct ProjectRunSnapshot: Sendable, Equatable {
    public let catalog: ProjectRunCatalog
    public let state: ProjectRunState
    public let environment: ProjectRunEnvironment

    public init(
        catalog: ProjectRunCatalog,
        state: ProjectRunState,
        environment: ProjectRunEnvironment
    ) {
        self.catalog = catalog
        self.state = state
        self.environment = environment
    }
}

public enum ProjectRunMutation: Sendable, Equatable {
    case analyze
    case selectDefaultTarget(String)
    case updateEnvironment(
        selectedToolchains: [String: String],
        customToolchains: [String: ProjectRunCustomToolchain],
        environmentVariables: [String: String]
    )
    case start(targetID: String)
    case stop(instanceID: String)
    case delete(instanceID: String)
}

public struct ProjectRunMutationResult: Sendable, Equatable {
    public let catalog: ProjectRunCatalog?
    public let state: ProjectRunState?
    public let environment: ProjectRunEnvironment?
    public let refreshIsComplete: Bool

    public init(
        catalog: ProjectRunCatalog? = nil,
        state: ProjectRunState? = nil,
        environment: ProjectRunEnvironment? = nil,
        refreshIsComplete: Bool
    ) {
        self.catalog = catalog
        self.state = state
        self.environment = environment
        self.refreshIsComplete = refreshIsComplete
    }
}

/// Application boundary for the project runtime settings screen.
///
/// The use case keeps operation commit and presentation refresh separate. Once
/// a start/stop/delete or preference write succeeds, a later snapshot failure
/// must not make the UI report that the operation itself failed.
public struct ProjectRunUseCase: Sendable {
    private let service: any ProjectRunServicing

    public init(service: any ProjectRunServicing) {
        self.service = service
    }

    public func changes(projectID: String) async -> AsyncStream<Void> {
        await service.changes(projectID: projectID)
    }

    public func snapshot(projectID: String) async throws -> ProjectRunSnapshot {
        async let catalog = service.fetchCatalog(projectID: projectID)
        async let state = service.fetchState(projectID: projectID)
        async let environment = service.fetchEnvironment(projectID: projectID)
        return try await .init(
            catalog: catalog,
            state: state,
            environment: environment
        )
    }

    public func state(projectID: String) async throws -> ProjectRunState {
        try await service.fetchState(projectID: projectID)
    }

    public func mutate(
        projectID: String,
        mutation: ProjectRunMutation
    ) async throws -> ProjectRunMutationResult {
        switch mutation {
        case .analyze:
            let catalog = try await service.analyze(projectID: projectID)
            async let state = try? service.fetchState(projectID: projectID)
            async let environment = try? service.fetchEnvironment(projectID: projectID)
            let refreshedState = await state
            let refreshedEnvironment = await environment
            return .init(
                catalog: catalog,
                state: refreshedState,
                environment: refreshedEnvironment,
                refreshIsComplete: refreshedState != nil && refreshedEnvironment != nil
            )

        case let .selectDefaultTarget(targetID):
            let catalog = try await service.setDefaultTarget(
                projectID: projectID,
                targetID: targetID
            )
            let environment = try? await service.fetchEnvironment(projectID: projectID)
            return .init(
                catalog: catalog,
                environment: environment,
                refreshIsComplete: environment != nil
            )

        case let .updateEnvironment(selectedToolchains, customToolchains, environmentVariables):
            let environment = try await service.updateEnvironment(
                projectID: projectID,
                selectedToolchains: selectedToolchains,
                customToolchains: customToolchains,
                environmentVariables: environmentVariables
            )
            return .init(environment: environment, refreshIsComplete: true)

        case let .start(targetID):
            try await service.start(projectID: projectID, targetID: targetID)
            let state = try? await service.fetchState(projectID: projectID)
            return .init(state: state, refreshIsComplete: state != nil)

        case let .stop(instanceID):
            try await service.stop(instanceID: instanceID)
            let state = try? await service.fetchState(projectID: projectID)
            return .init(state: state, refreshIsComplete: state != nil)

        case let .delete(instanceID):
            try await service.delete(instanceID: instanceID)
            let state = try? await service.fetchState(projectID: projectID)
            return .init(state: state, refreshIsComplete: state != nil)
        }
    }
}
