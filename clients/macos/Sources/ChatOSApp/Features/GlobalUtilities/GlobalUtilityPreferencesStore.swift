import ChatOSConnector
import ChatOSCore
import Foundation

private struct GlobalUtilityPreferencesState: Codable, Equatable, Sendable {
    var isEnabled = false
    var hasAcknowledgedShortcutConflicts = false
    var screenshotEnabled = true
    var recordingEnabled = true
    var clipboardEnabled = true
    var quickSearchEnabled = true
    var hotKeys = Dictionary(uniqueKeysWithValues: GlobalUtilityAction.allCases.map {
        ($0.rawValue, $0.defaultHotKey)
    })
}

@MainActor
final class GlobalUtilityPreferencesStore: ObservableObject {
    @Published var isEnabled = false { didSet { preferenceDidChange() } }
    @Published var hasAcknowledgedShortcutConflicts = false {
        didSet { preferenceDidChange() }
    }
    @Published var screenshotEnabled = true { didSet { preferenceDidChange() } }
    @Published var recordingEnabled = true { didSet { preferenceDidChange() } }
    @Published var clipboardEnabled = true { didSet { preferenceDidChange() } }
    @Published var quickSearchEnabled = true { didSet { preferenceDidChange() } }
    @Published private(set) var hotKeys = Dictionary(
        uniqueKeysWithValues: GlobalUtilityAction.allCases.map { ($0, $0.defaultHotKey) }
    )
    @Published private(set) var configurationRevision = UUID()
    @Published private(set) var isStorageReady = false
    @Published private(set) var persistenceError: String?

    private let persistence: NativeLocalClientSettingStore<GlobalUtilityPreferencesState>
    private var activeOwnerUserID: String?
    private var persistedState = GlobalUtilityPreferencesState()
    private var isApplyingState = false
    private var mutation: UInt64 = 0
    private var persistedMutation: UInt64 = 0
    private var saveTask: Task<Void, Never>?

    init(accountSession: any NativeLocalAgentAccountSessionAccess) {
        do {
            persistence = try NativeLocalClientSettingStore(
                key: "global_utilities.preferences",
                accountSession: accountSession
            )
        } catch {
            preconditionFailure("Global utility preference storage key is invalid")
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
                defaultValue: GlobalUtilityPreferencesState()
            )
            guard activeOwnerUserID == ownerUserID else { return }
            persistedState = state
            persistedMutation = 0
            mutation = 0
            apply(state)
            isStorageReady = true
        } catch {
            guard activeOwnerUserID == ownerUserID else { return }
            apply(GlobalUtilityPreferencesState())
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
        apply(GlobalUtilityPreferencesState())
        await persistence.reset()
    }

    func hotKey(for action: GlobalUtilityAction) -> GlobalHotKey {
        hotKeys[action] ?? action.defaultHotKey
    }

    func isActionEnabled(_ action: GlobalUtilityAction) -> Bool {
        switch action {
        case .screenshot: screenshotEnabled
        case .screenRecording: recordingEnabled
        case .clipboardHistory: clipboardEnabled
        case .quickSearch: quickSearchEnabled
        }
    }

    func setHotKey(_ hotKey: GlobalHotKey, for action: GlobalUtilityAction) {
        guard hotKey.isValid else { return }
        hotKeys[action] = hotKey
        configurationDidChange()
    }

    func setActionEnabled(_ enabled: Bool, for action: GlobalUtilityAction) {
        switch action {
        case .screenshot: screenshotEnabled = enabled
        case .screenRecording: recordingEnabled = enabled
        case .clipboardHistory: clipboardEnabled = enabled
        case .quickSearch: quickSearchEnabled = enabled
        }
    }

    func restoreDefaults() {
        let acknowledgement = hasAcknowledgedShortcutConflicts
        let enabled = isEnabled
        var state = GlobalUtilityPreferencesState()
        state.isEnabled = enabled
        state.hasAcknowledgedShortcutConflicts = acknowledgement
        apply(state)
        configurationDidChange()
    }

    private func configurationDidChange() {
        configurationRevision = UUID()
        preferenceDidChange()
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
        _ state: GlobalUtilityPreferencesState,
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

    private func currentState() -> GlobalUtilityPreferencesState {
        GlobalUtilityPreferencesState(
            isEnabled: isEnabled,
            hasAcknowledgedShortcutConflicts: hasAcknowledgedShortcutConflicts,
            screenshotEnabled: screenshotEnabled,
            recordingEnabled: recordingEnabled,
            clipboardEnabled: clipboardEnabled,
            quickSearchEnabled: quickSearchEnabled,
            hotKeys: Dictionary(uniqueKeysWithValues: GlobalUtilityAction.allCases.map {
                ($0.rawValue, hotKey(for: $0))
            })
        )
    }

    private func apply(_ state: GlobalUtilityPreferencesState) {
        isApplyingState = true
        isEnabled = state.isEnabled
        hasAcknowledgedShortcutConflicts = state.hasAcknowledgedShortcutConflicts
        screenshotEnabled = state.screenshotEnabled
        recordingEnabled = state.recordingEnabled
        clipboardEnabled = state.clipboardEnabled
        quickSearchEnabled = state.quickSearchEnabled
        hotKeys = Dictionary(uniqueKeysWithValues: GlobalUtilityAction.allCases.map { action in
            (action, state.hotKeys[action.rawValue] ?? action.defaultHotKey)
        })
        configurationRevision = UUID()
        isApplyingState = false
    }
}
