import AppKit
import ChatOSCore
import Combine
import Foundation

enum PetStatusRefreshPolicy {
    static let activeInterval = Duration.seconds(20)

    static func shouldRun(isMonitoring: Bool, activeWorkCount: Int) -> Bool {
        isMonitoring && activeWorkCount > 0
    }
}

@MainActor
final class PetOverlayCoordinator {
    private weak var model: AppModel?
    private let store: PetOverlayStore
    private let preferences: PetPreferencesStore
    private let windowController: PetOverlayWindowController
    private var realtimeTask: Task<Void, Never>?
    private var recoveryTask: Task<Void, Never>?
    private var refreshTask: Task<Void, Never>?
    private var recoveryRequestedWhileRunning = false
    private var recoveryGeneration: UInt64 = 0
    private var cancellables = Set<AnyCancellable>()
    private var isAuthenticated = false
    private var isMonitoring = false

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
        store.onDisposition = { [weak self, weak model] activity, disposition in
            guard activity.inboxID != nil else { return }
            Task {
                do {
                    try await model?.applyPetActivityDisposition(disposition, to: activity)
                } catch {
                    await MainActor.run {
                        self?.store.restoreDismissal(activity)
                        self?.store.apply(.upsert(activity))
                        self?.recoverCloudState()
                    }
                }
            }
        }
        bind(model: model)
    }

    deinit {
        realtimeTask?.cancel()
        recoveryTask?.cancel()
        refreshTask?.cancel()
    }

    func openFile(_ request: PetFileOpenRequest) {
        windowController.openFile(request)
    }

    func openTranslationImage(data: Data, suggestedName: String) {
        windowController.openTranslationImage(data: data, suggestedName: suggestedName)
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

        Publishers.CombineLatest(model.$projects, model.$contacts)
            .debounce(for: .milliseconds(250), scheduler: RunLoop.main)
            .sink { [weak self] _, _ in
                self?.recoverCloudState()
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

        NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.recoverCloudState() }
            .store(in: &cancellables)

        NSWorkspace.shared.notificationCenter.publisher(for: NSWorkspace.didWakeNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.recoverCloudState() }
            .store(in: &cancellables)

        store.$presentation
            .map(\.activeWorkCount)
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.updateStatusRefresh() }
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

        isAuthenticated = authenticated
        let shouldMonitor = authenticated && enabled
        if shouldMonitor != isMonitoring {
            isMonitoring = shouldMonitor
            if shouldMonitor {
                startRealtime()
                recoverCloudState()
                updateStatusRefresh()
            } else {
                realtimeTask?.cancel()
                realtimeTask = nil
                recoveryTask?.cancel()
                recoveryTask = nil
                recoveryGeneration &+= 1
                recoveryRequestedWhileRunning = false
                refreshTask?.cancel()
                refreshTask = nil
            }
        }
        if !authenticated {
            if !store.activities.isEmpty {
                store.clear()
            }
        }
        windowController.setVisible(authenticated && enabled)
    }

    private func startRealtime() {
        guard realtimeTask == nil,
              let realtime = model?.petActivityService else { return }
        realtimeTask = Task { [weak self] in
            let stream = await realtime.petActivityEvents()
            do {
                for try await event in stream {
                    guard let self, !Task.isCancelled else { return }
                    if event == .reconcile {
                        self.recoverCloudState()
                        continue
                    }
                    if self.shouldApply(event) {
                        self.store.apply(event)
                    }
                }
            } catch {
                guard let self, !Task.isCancelled else { return }
                self.store.apply(.upsert(PetActivity(
                    id: "pet-realtime-connection",
                    source: .chat,
                    kind: .failed,
                    title: "全局事件连接已中断",
                    detail: "ChatOS 会自动尝试重新连接",
                    expiresAt: Date().addingTimeInterval(8)
                )))
            }
            self?.realtimeTask = nil
        }
    }

    private func startStatusRefresh() {
        guard refreshTask == nil,
              PetStatusRefreshPolicy.shouldRun(
                  isMonitoring: isMonitoring,
                  activeWorkCount: store.presentation.activeWorkCount
              ) else { return }
        refreshTask = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    try await Task.sleep(for: PetStatusRefreshPolicy.activeInterval)
                } catch {
                    return
                }
                guard let self,
                      !Task.isCancelled,
                      PetStatusRefreshPolicy.shouldRun(
                          isMonitoring: self.isMonitoring,
                          activeWorkCount: self.store.presentation.activeWorkCount
                      ) else { return }
                self.recoverCloudState()
            }
        }
    }

    private func updateStatusRefresh() {
        guard PetStatusRefreshPolicy.shouldRun(
            isMonitoring: isMonitoring,
            activeWorkCount: store.presentation.activeWorkCount
        ) else {
            refreshTask?.cancel()
            refreshTask = nil
            return
        }
        startStatusRefresh()
    }

    private func recoverCloudState() {
        guard isAuthenticated, let model else { return }
        // Reconcile signals can arrive faster than the local host can answer. Cancelling the
        // in-flight authoritative read for every signal can starve reconciliation indefinitely,
        // leaving terminal Runs displayed as active. Coalesce those signals into one guaranteed
        // follow-up read instead.
        guard recoveryTask == nil else {
            recoveryRequestedWhileRunning = true
            return
        }
        recoveryRequestedWhileRunning = false
        recoveryGeneration &+= 1
        let generation = recoveryGeneration
        let sources: [PetActivitySource] = [
            .askUserPrompt,
            .chat,
            .taskBoard,
            .taskExecution,
        ]
        let expectedVersions = store.versions(for: sources)
        recoveryTask = Task { [weak self, weak model] in
            do {
                guard let model else { return }
                let activities = try await model.recoverPetActivities()
                guard let self, !Task.isCancelled else { return }
                let visibleActivities = activities.filter {
                    self.shouldApply(.upsert($0))
                }
                self.store.reconcileActivities(
                    visibleActivities,
                    sources: sources,
                    expectedVersions: expectedVersions
                )
            } catch {
                guard !Task.isCancelled else { return }
            }
            self?.finishRecovery(generation: generation)
        }
    }

    private func finishRecovery(generation: UInt64) {
        guard generation == recoveryGeneration else { return }
        recoveryTask = nil
        guard recoveryRequestedWhileRunning, isMonitoring else { return }
        recoveryRequestedWhileRunning = false
        recoverCloudState()
    }

    private func shouldApply(_ event: PetActivityEvent) -> Bool {
        guard case let .upsert(activity) = event else { return true }
        if !preferences.showProcess, activity.kind == .working {
            return false
        }
        if !preferences.showCompletions, activity.kind == .succeeded {
            return false
        }
        return true
    }
}
