import ChatOSCore
import Combine
import Foundation

@MainActor
final class PetOverlayCoordinator {
    private weak var model: AppModel?
    private let store: PetOverlayStore
    private let preferences: PetPreferencesStore
    private let windowController: PetOverlayWindowController
    private var localTaskUpdateTask: Task<Void, Never>?
    private var cancellables = Set<AnyCancellable>()
    private var isAuthenticated = false

    init(model: AppModel, store: PetOverlayStore, preferences: PetPreferencesStore) {
        self.model = model
        self.store = store
        self.preferences = preferences
        self.windowController = PetOverlayWindowController(
            model: model,
            store: store,
            preferences: preferences,
            approvalViewModel: model.localConnectorControl,
            onOpen: { [weak model] activity in
                model?.openPetActivity(activity)
            },
            onRetry: { [weak model] activity, instruction in
                guard let model else { return }
                try await model.retryPetActivity(activity, instruction: instruction)
            },
            onCancel: { [weak model] activity in
                guard let model else { return }
                try await model.cancelPetActivity(activity)
            },
            onLoadTask: { [weak model] activity in
                guard let model else { throw CancellationError() }
                return try await model.loadPetTask(activity)
            },
            onLoadPrompt: { [weak model] activity in
                guard let model else { throw CancellationError() }
                return try await model.loadPetAskUserPrompt(activity)
            },
            onSubmitPrompt: { [weak model] prompt, submission in
                guard let model else { return }
                try await model.submitPetAskUserPrompt(prompt, submission: submission)
            },
            onCancelPrompt: { [weak model] prompt in
                guard let model else { return }
                try await model.cancelPetAskUserPrompt(prompt)
            }
        )

        store.startExpirationMonitoring()
        bind(model: model)
    }

    deinit {
        localTaskUpdateTask?.cancel()
    }

    func openFile(_ request: PetFileOpenRequest) {
        windowController.openFile(request)
    }

    private func bind(model: AppModel) {
        Publishers.CombineLatest(
            model.authentication.$phase.removeDuplicates(),
            preferences.$isEnabled.removeDuplicates()
        )
        .receive(on: RunLoop.main)
        .sink { [weak self] phase, enabled in
            self?.applyVisibility(phase: phase, enabled: enabled)
        }
        .store(in: &cancellables)

        model.localConnectorControl.$pendingApprovals
            .receive(on: RunLoop.main)
            .sink { [weak store] approvals in
                store?.replaceApprovals(approvals)
            }
            .store(in: &cancellables)

        model.localConnectorControl.$latestApprovalEvent
            .compactMap { $0 }
            .receive(on: RunLoop.main)
            .sink { [weak store] event in
                store?.showApprovalEvent(event)
            }
            .store(in: &cancellables)

        preferences.$showProcess
            .removeDuplicates()
            .filter { !$0 }
            .receive(on: RunLoop.main)
            .sink { [weak store] _ in store?.removeProcessActivities() }
            .store(in: &cancellables)

        preferences.$showCompletions
            .removeDuplicates()
            .filter { !$0 }
            .receive(on: RunLoop.main)
            .sink { [weak store] _ in store?.removeCompletionActivities() }
            .store(in: &cancellables)

        NotificationCenter.default.publisher(for: .chatOSPetOpenFileRequested)
            .compactMap { $0.object as? PetFilePresentationRequest }
            .receive(on: RunLoop.main)
            .sink { [weak model] request in
                model?.openPetFile(
                    path: request.path,
                    targetLine: request.targetLine,
                    mode: request.prefersEditing ? .edit : .preview
                )
            }
            .store(in: &cancellables)
    }

    private func applyVisibility(
        phase: AuthenticationViewModel.Phase,
        enabled: Bool
    ) {
        let authenticated: Bool
        if case .authenticated = phase {
            authenticated = true
        } else {
            authenticated = false
        }

        if authenticated != isAuthenticated {
            isAuthenticated = authenticated
            if authenticated {
                startLocalTaskUpdates()
            } else {
                localTaskUpdateTask?.cancel()
                localTaskUpdateTask = nil
                store.clear()
            }
        }
        windowController.setVisible(authenticated && enabled)
    }

    private func startLocalTaskUpdates() {
        guard localTaskUpdateTask == nil, let model else { return }
        let taskStore = model.localAgentTaskStateStore
        localTaskUpdateTask = Task { [weak self] in
            guard let self else { return }
            let stream = await taskStore.localAgentTaskUpdates()
            await self.reconcileLocalTasks(from: taskStore)
            for await _ in stream {
                guard !Task.isCancelled else { return }
                await self.reconcileLocalTasks(from: taskStore)
            }
        }
    }

    private func reconcileLocalTasks(from taskStore: LocalAgentTaskStateStore) async {
        let sources: [PetActivitySource] = [.askUserPrompt, .taskRunner]
        let expectedVersions = store.versions(for: sources)
        let states = await taskStore.localAgentTasks(sessionID: nil)
        guard !Task.isCancelled else { return }
        let activities = PetActivityRecoveryMapper.activities(from: states).filter(shouldApply)
        store.reconcileActivities(
            activities,
            sources: sources,
            expectedVersions: expectedVersions
        )
    }

    private func shouldApply(_ activity: PetActivity) -> Bool {
        if !preferences.showProcess,
           activity.kind == .working || activity.kind == .reviewing {
            return false
        }
        if !preferences.showCompletions, activity.kind == .succeeded {
            return false
        }
        return true
    }
}
