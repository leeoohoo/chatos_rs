import ChatOSCore
import Foundation

@MainActor
final class ProjectRunSettingsViewModel: ObservableObject {
    enum Notice: Equatable {
        case analyzed
        case defaultTargetSaved
        case environmentSaved
        case instanceStarted
        case stopRequested
        case instanceDeleted
    }

    struct EnvironmentVariableDraft: Identifiable, Equatable {
        let id: UUID
        var key: String
        var value: String
        init(id: UUID = UUID(), key: String = "", value: String = "") {
            self.id = id; self.key = key; self.value = value
        }
    }

    @Published private(set) var catalog: ProjectRunCatalog?
    @Published private(set) var state: ProjectRunState?
    @Published private(set) var environment: ProjectRunEnvironment?
    @Published var selectedTargetID: String?
    @Published var selectedInstanceID: String?
    @Published var selectedToolchains: [String: String] = [:]
    @Published var environmentVariables: [EnvironmentVariableDraft] = []
    @Published private(set) var isLoading = false
    @Published private(set) var isMutating = false
    @Published private(set) var notice: Notice?
    @Published private(set) var errorMessage: String?

    let projectID: String
    private let service: any ProjectRunServicing

    init(projectID: String, service: any ProjectRunServicing) {
        self.projectID = projectID
        self.service = service
    }

    var targets: [ProjectRunTarget] { catalog?.targets ?? [] }
    var selectedTarget: ProjectRunTarget? { targets.first(where: { $0.id == selectedTargetID }) }
    var instances: [ProjectRunInstance] { state?.instances ?? [] }
    var selectedInstance: ProjectRunInstance? { instances.first(where: { $0.id == selectedInstanceID }) }

    func load() async {
        isLoading = true
        errorMessage = nil
        do {
            async let catalog = service.fetchCatalog(projectID: projectID)
            async let state = service.fetchState(projectID: projectID)
            async let environment = service.fetchEnvironment(projectID: projectID)
            let values = try await (catalog, state, environment)
            apply(catalog: values.0, state: values.1, environment: values.2)
        } catch {
            errorMessage = error.localizedDescription
        }
        isLoading = false
    }

    func monitorRuns() async {
        while !Task.isCancelled {
            try? await Task.sleep(for: .seconds(1))
            guard !Task.isCancelled, !instances.isEmpty else { continue }
            if let refreshed = try? await service.fetchState(projectID: projectID) {
                state = refreshed
                if selectedInstanceID == nil { selectedInstanceID = refreshed.instances.first?.id }
            }
        }
    }

    func analyze() async {
        await mutate(successNotice: .analyzed) {
            let catalog = try await service.analyze(projectID: projectID)
            self.catalog = catalog
            selectedTargetID = catalog.defaultTargetID ?? catalog.targets.first?.id
            environment = try await service.fetchEnvironment(projectID: projectID)
            applyEnvironmentDrafts()
        }
    }

    func selectTarget(_ id: String?) async {
        selectedTargetID = id
        guard let id else { return }
        await mutate(successNotice: .defaultTargetSaved) {
            catalog = try await service.setDefaultTarget(projectID: projectID, targetID: id)
            environment = try await service.fetchEnvironment(projectID: projectID)
            applyEnvironmentDrafts()
        }
    }

    func saveEnvironment() async {
        let variables = Dictionary(
            environmentVariables.compactMap { draft -> (String, String)? in
                let key = draft.key.trimmingCharacters(in: .whitespacesAndNewlines)
                return key.isEmpty ? nil : (key, draft.value)
            },
            uniquingKeysWith: { _, latest in latest }
        )
        await mutate(successNotice: .environmentSaved) {
            environment = try await service.updateEnvironment(
                projectID: projectID,
                selectedToolchains: selectedToolchains,
                customToolchains: environment?.customToolchains ?? [:],
                environmentVariables: variables
            )
            applyEnvironmentDrafts()
        }
    }

    func addEnvironmentVariable() { environmentVariables.append(.init()) }
    func removeEnvironmentVariable(id: UUID) { environmentVariables.removeAll(where: { $0.id == id }) }

    func start() async {
        guard let selectedTargetID else { return }
        await mutate(successNotice: .instanceStarted) {
            try await service.start(projectID: projectID, targetID: selectedTargetID)
            try await Task.sleep(for: .milliseconds(450))
            state = try await service.fetchState(projectID: projectID)
            selectedInstanceID = state?.instances.first?.id
        }
    }

    func stop(instanceID: String? = nil) async {
        guard let instanceID = instanceID ?? selectedInstanceID else { return }
        await mutate(successNotice: .stopRequested) {
            try await service.stop(instanceID: instanceID)
            try await Task.sleep(for: .milliseconds(300))
            state = try await service.fetchState(projectID: projectID)
        }
    }

    func deleteInstance(instanceID: String? = nil) async {
        guard let instanceID = instanceID ?? selectedInstanceID else { return }
        await mutate(successNotice: .instanceDeleted) {
            try await service.delete(instanceID: instanceID)
            state = try await service.fetchState(projectID: projectID)
            self.selectedInstanceID = state?.instances.first?.id
        }
    }

    func dismissError() { errorMessage = nil }
    func dismissMessage() { notice = nil }

    private func apply(catalog: ProjectRunCatalog, state: ProjectRunState, environment: ProjectRunEnvironment) {
        self.catalog = catalog
        self.state = state
        self.environment = environment
        selectedTargetID = catalog.defaultTargetID ?? catalog.targets.first(where: \.isDefault)?.id ?? catalog.targets.first?.id
        selectedInstanceID = state.instances.first?.id
        applyEnvironmentDrafts()
    }

    private func applyEnvironmentDrafts() {
        selectedToolchains = environment?.selectedToolchains ?? [:]
        environmentVariables = (environment?.environmentVariables ?? [:])
            .sorted(by: { $0.key < $1.key })
            .map { .init(key: $0.key, value: $0.value) }
    }

    private func mutate(successNotice: Notice, operation: () async throws -> Void) async {
        isMutating = true
        errorMessage = nil
        notice = nil
        do {
            try await operation()
            notice = successNotice
        } catch {
            errorMessage = error.localizedDescription
        }
        isMutating = false
    }
}
