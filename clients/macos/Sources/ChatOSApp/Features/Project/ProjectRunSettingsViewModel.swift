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

    enum RefreshWarning: Equatable {
        case committedWithoutRefresh
    }

    struct EnvironmentVariableDraft: Identifiable, Equatable {
        let id: UUID
        var key: String
        var value: String

        init(id: UUID = UUID(), key: String = "", value: String = "") {
            self.id = id
            self.key = key
            self.value = value
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
    @Published private(set) var refreshWarning: RefreshWarning?
    @Published private(set) var isFavoriteProject: Bool

    let projectID: String
    private let useCase: ProjectRunUseCase
    private let favoriteUseCase: ProjectFavoriteUseCase
    private var lifecycle = ProjectRunLifecycleState()
    private var monitorSequence: UInt64 = 0

    init(
        projectID: String,
        service: any ProjectRunServicing,
        favoritePreferences: any ProjectFavoritePreferences
    ) {
        self.projectID = projectID
        self.useCase = ProjectRunUseCase(service: service)
        let favoriteUseCase = ProjectFavoriteUseCase(preferences: favoritePreferences)
        self.favoriteUseCase = favoriteUseCase
        self.isFavoriteProject = favoriteUseCase.isFavorite(projectID: projectID)
    }

    var targets: [ProjectRunTarget] { catalog?.targets ?? [] }
    var selectedTarget: ProjectRunTarget? { targets.first(where: { $0.id == selectedTargetID }) }
    var instances: [ProjectRunInstance] { state?.instances ?? [] }
    var selectedInstance: ProjectRunInstance? { instances.first(where: { $0.id == selectedInstanceID }) }

    func load() async {
        guard let request = lifecycle.beginLoad(projectID: projectID) else { return }
        publishLifecycle()
        errorMessage = nil
        refreshWarning = nil
        let useCase = useCase
        do {
            let snapshot = try await useCase.snapshot(projectID: projectID)
            guard !Task.isCancelled, lifecycle.acceptsLoad(request) else { return }
            applyCatalog(snapshot.catalog)
            environment = snapshot.environment
            applyEnvironmentDrafts()
            if lifecycle.acceptsState(request) {
                applyState(snapshot.state)
            }
        } catch {
            guard !Task.isCancelled, lifecycle.acceptsLoad(request) else { return }
            errorMessage = error.localizedDescription
        }
        lifecycle.finishLoad(request)
        publishLifecycle()
    }

    func monitorRuns() async {
        monitorSequence &+= 1
        let monitorID = monitorSequence
        let useCase = useCase

        // Subscribe before loading so a fast process exit cannot land in the
        // gap between the initial snapshot and realtime observation.
        let changes = await useCase.changes(projectID: projectID)
        guard !Task.isCancelled, monitorID == monitorSequence else { return }
        await load()
        guard !Task.isCancelled, monitorID == monitorSequence else { return }

        for await _ in changes {
            guard !Task.isCancelled, monitorID == monitorSequence else { return }
            await refreshState(using: useCase)
        }
    }

    func deactivate() {
        monitorSequence &+= 1
        lifecycle.reset()
        publishLifecycle()
    }

    func analyze() async {
        await mutate(.analyze, successNotice: .analyzed)
    }

    func selectTarget(_ id: String?) async {
        selectedTargetID = id
        guard let id else { return }
        await mutate(.selectDefaultTarget(id), successNotice: .defaultTargetSaved)
    }

    func saveEnvironment() async {
        let draft = environmentDraftSnapshot()
        await mutate(
            .updateEnvironment(
                selectedToolchains: draft.selectedToolchains,
                customToolchains: environment?.customToolchains ?? [:],
                environmentVariables: draft.environmentVariables
            ),
            successNotice: .environmentSaved,
            submittedEnvironmentDraft: draft
        )
    }

    func addEnvironmentVariable() { environmentVariables.append(.init()) }
    func removeEnvironmentVariable(id: UUID) { environmentVariables.removeAll(where: { $0.id == id }) }

    func setFavoriteProject(_ isFavorite: Bool) {
        isFavoriteProject = favoriteUseCase.setFavorite(isFavorite, projectID: projectID)
    }

    func start() async {
        guard let selectedTargetID else { return }
        await mutate(.start(targetID: selectedTargetID), successNotice: .instanceStarted)
    }

    func stop(instanceID: String? = nil) async {
        guard let instanceID = instanceID ?? selectedInstanceID else { return }
        await mutate(.stop(instanceID: instanceID), successNotice: .stopRequested)
    }

    func deleteInstance(instanceID: String? = nil) async {
        guard let instanceID = instanceID ?? selectedInstanceID else { return }
        await mutate(.delete(instanceID: instanceID), successNotice: .instanceDeleted)
    }

    func dismissError() { errorMessage = nil }
    func dismissMessage() {
        notice = nil
        refreshWarning = nil
    }

    private func refreshState(using useCase: ProjectRunUseCase) async {
        guard let request = lifecycle.beginStateRefresh(projectID: projectID) else { return }
        do {
            let state = try await useCase.state(projectID: projectID)
            guard !Task.isCancelled, lifecycle.acceptsState(request) else { return }
            applyState(state)
        } catch {
            // Realtime refresh is a reconciliation hint. Preserve the last
            // trusted state; explicit loads and mutations surface errors.
        }
        lifecycle.finishStateRefresh(request)
    }

    private func mutate(
        _ mutation: ProjectRunMutation,
        successNotice: Notice,
        submittedEnvironmentDraft: EnvironmentDraftSnapshot? = nil
    ) async {
        guard let request = lifecycle.beginMutation(projectID: projectID) else { return }
        publishLifecycle()
        errorMessage = nil
        refreshWarning = nil
        notice = nil
        let useCase = useCase
        let environmentDraftAtStart = submittedEnvironmentDraft ?? environmentDraftSnapshot()

        do {
            let result = try await useCase.mutate(projectID: projectID, mutation: mutation)
            guard lifecycle.acceptsMutation(request) else { return }
            applyMutationResult(
                result,
                mutation: mutation,
                environmentDraftAtStart: environmentDraftAtStart
            )
            notice = successNotice
            if !result.refreshIsComplete {
                refreshWarning = .committedWithoutRefresh
            }
        } catch {
            guard lifecycle.acceptsMutation(request) else { return }
            let operationError = error.localizedDescription
            if let recovered = try? await useCase.snapshot(projectID: projectID),
               lifecycle.acceptsMutation(request) {
                apply(
                    recovered,
                    replaceEnvironmentDrafts: environmentDraftSnapshot() == environmentDraftAtStart
                )
            }
            guard lifecycle.acceptsMutation(request) else { return }
            errorMessage = operationError
        }

        lifecycle.finishMutation(request)
        publishLifecycle()
    }

    private func apply(
        _ snapshot: ProjectRunSnapshot,
        replaceEnvironmentDrafts: Bool = true
    ) {
        applyCatalog(snapshot.catalog)
        applyState(snapshot.state)
        environment = snapshot.environment
        if replaceEnvironmentDrafts { applyEnvironmentDrafts() }
    }

    private func applyMutationResult(
        _ result: ProjectRunMutationResult,
        mutation: ProjectRunMutation,
        environmentDraftAtStart: EnvironmentDraftSnapshot
    ) {
        if let catalog = result.catalog {
            self.catalog = catalog
            if case let .selectDefaultTarget(targetID) = mutation {
                if selectedTargetID == targetID {
                    selectedTargetID = catalog.defaultTargetID ?? targetID
                }
            } else if targets.contains(where: { $0.id == selectedTargetID }) == false {
                selectedTargetID = preferredTargetID(in: catalog)
            }
        }
        if let state = result.state { applyState(state) }
        if let environment = result.environment {
            self.environment = environment
            if environmentDraftSnapshot() == environmentDraftAtStart {
                applyEnvironmentDrafts()
            }
        }
    }

    private func applyCatalog(_ catalog: ProjectRunCatalog) {
        self.catalog = catalog
        selectedTargetID = preferredTargetID(in: catalog)
    }

    private func preferredTargetID(in catalog: ProjectRunCatalog) -> String? {
        catalog.defaultTargetID
            ?? catalog.targets.first(where: \.isDefault)?.id
            ?? catalog.targets.first?.id
    }

    private func applyState(_ state: ProjectRunState) {
        self.state = state
        if let selectedInstanceID,
           state.instances.contains(where: { $0.id == selectedInstanceID }) {
            return
        }
        selectedInstanceID = state.instances.first?.id
    }

    private func applyEnvironmentDrafts() {
        selectedToolchains = environment?.selectedToolchains ?? [:]
        environmentVariables = (environment?.environmentVariables ?? [:])
            .sorted(by: { $0.key < $1.key })
            .map { .init(key: $0.key, value: $0.value) }
    }

    private func environmentDraftSnapshot() -> EnvironmentDraftSnapshot {
        .init(
            selectedToolchains: selectedToolchains,
            environmentVariables: Dictionary(
                environmentVariables.compactMap { draft -> (String, String)? in
                    let key = draft.key.trimmingCharacters(in: .whitespacesAndNewlines)
                    return key.isEmpty ? nil : (key, draft.value)
                },
                uniquingKeysWith: { _, latest in latest }
            )
        )
    }

    private func publishLifecycle() {
        isLoading = lifecycle.isLoading
        isMutating = lifecycle.isMutating
    }

    private struct EnvironmentDraftSnapshot: Equatable {
        let selectedToolchains: [String: String]
        let environmentVariables: [String: String]
    }
}
