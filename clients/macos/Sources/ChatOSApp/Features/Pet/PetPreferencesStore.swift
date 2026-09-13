import ChatOSConnector
import Foundation

private struct PetPreferencesState: Codable, Equatable, Sendable {
    var isEnabled = true
    var size = 104.0
    var showProcess = true
    var showCompletions = true
    var showAcrossSpaces = true
    var favoriteProjectIDs: Set<String> = []
}

@MainActor
final class PetPreferencesStore: ObservableObject {
    @Published var isEnabled = false { didSet { preferenceDidChange() } }
    @Published var size = 104.0 {
        didSet {
            let normalized = min(180, max(72, size))
            if normalized != size {
                size = normalized
            } else {
                preferenceDidChange()
            }
        }
    }
    @Published var showProcess = true { didSet { preferenceDidChange() } }
    @Published var showCompletions = true { didSet { preferenceDidChange() } }
    @Published var showAcrossSpaces = true { didSet { preferenceDidChange() } }
    @Published private(set) var favoriteProjectIDs: Set<String> = []
    @Published private(set) var resetPositionRequestID = UUID()
    @Published private(set) var isStorageReady = false
    @Published private(set) var persistenceError: String?

    private let persistence: NativeLocalClientSettingStore<PetPreferencesState>
    private var activeOwnerUserID: String?
    private var persistedState = PetPreferencesState()
    private var isApplyingState = false
    private var mutation: UInt64 = 0
    private var persistedMutation: UInt64 = 0
    private var saveTask: Task<Void, Never>?

    init(accountSession: any NativeLocalAgentAccountSessionAccess) {
        do {
            persistence = try NativeLocalClientSettingStore(
                key: "pet.preferences",
                accountSession: accountSession
            )
        } catch {
            preconditionFailure("Pet preference storage key is invalid")
        }
    }

    func activate(ownerUserID: String) async {
        saveTask?.cancel()
        activeOwnerUserID = ownerUserID
        isStorageReady = false
        persistenceError = nil
        await persistence.reset()
        do {
            let state = try await persistence.load(
                ownerUserID: ownerUserID,
                defaultValue: PetPreferencesState()
            )
            guard activeOwnerUserID == ownerUserID else { return }
            persistedState = state
            persistedMutation = 0
            mutation = 0
            apply(state)
            isStorageReady = true
        } catch {
            guard activeOwnerUserID == ownerUserID else { return }
            applyInactiveState()
            persistenceError = error.localizedDescription
        }
    }

    func retryLoading() {
        guard let ownerUserID = activeOwnerUserID else { return }
        Task { await activate(ownerUserID: ownerUserID) }
    }

    func flush() async {
        saveTask?.cancel()
        guard isStorageReady,
              mutation > persistedMutation,
              let ownerUserID = activeOwnerUserID else { return }
        let next = currentState()
        await persist(next, ownerUserID: ownerUserID, mutation: mutation)
    }

    func deactivate() async {
        saveTask?.cancel()
        saveTask = nil
        activeOwnerUserID = nil
        isStorageReady = false
        persistenceError = nil
        mutation = 0
        persistedMutation = 0
        applyInactiveState()
        await persistence.reset()
    }

    func isFavorite(projectID: String) -> Bool {
        favoriteProjectIDs.contains(projectID)
    }

    func setFavorite(_ isFavorite: Bool, projectID: String) {
        let normalizedID = projectID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedID.isEmpty else { return }
        if isFavorite {
            favoriteProjectIDs.insert(normalizedID)
        } else {
            favoriteProjectIDs.remove(normalizedID)
        }
        preferenceDidChange()
    }

    func requestPositionReset() {
        resetPositionRequestID = UUID()
    }

    private func preferenceDidChange() {
        guard !isApplyingState, isStorageReady, let ownerUserID = activeOwnerUserID else {
            return
        }
        mutation &+= 1
        let expectedMutation = mutation
        saveTask?.cancel()
        saveTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(80))
            guard !Task.isCancelled, let self else { return }
            let next = currentState()
            await persist(next, ownerUserID: ownerUserID, mutation: expectedMutation)
        }
    }

    private func persist(
        _ state: PetPreferencesState,
        ownerUserID: String,
        mutation: UInt64
    ) async {
        do {
            let committed = try await persistence.saveLatest(
                ownerUserID: ownerUserID,
                value: state,
                mutation: mutation
            )
            guard activeOwnerUserID == ownerUserID else { return }
            if committed, mutation >= persistedMutation {
                persistedMutation = mutation
                persistedState = state
            }
            if mutation == self.mutation {
                persistenceError = nil
            }
        } catch {
            guard activeOwnerUserID == ownerUserID, mutation == self.mutation else { return }
            apply(persistedState)
            isStorageReady = false
            persistenceError = error.localizedDescription
        }
    }

    private func currentState() -> PetPreferencesState {
        PetPreferencesState(
            isEnabled: isEnabled,
            size: size,
            showProcess: showProcess,
            showCompletions: showCompletions,
            showAcrossSpaces: showAcrossSpaces,
            favoriteProjectIDs: favoriteProjectIDs
        )
    }

    private func apply(_ state: PetPreferencesState) {
        isApplyingState = true
        isEnabled = state.isEnabled
        size = state.size
        showProcess = state.showProcess
        showCompletions = state.showCompletions
        showAcrossSpaces = state.showAcrossSpaces
        favoriteProjectIDs = state.favoriteProjectIDs
        isApplyingState = false
    }

    private func applyInactiveState() {
        var state = PetPreferencesState()
        state.isEnabled = false
        state.favoriteProjectIDs = []
        apply(state)
    }
}
