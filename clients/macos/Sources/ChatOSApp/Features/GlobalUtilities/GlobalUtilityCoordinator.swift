import ChatOSCore
import Combine
import Foundation

@MainActor
final class GlobalUtilityCoordinator {
    let preferences: GlobalUtilityPreferencesStore
    let hotKeys: GlobalHotKeyService

    private let screenshotCoordinator: ScreenshotCoordinator
    private let quickSearchCoordinator: QuickSearchCoordinator
    private let clipboardCoordinator: ClipboardHistoryCoordinator
    private let screenRecordingCoordinator: ScreenRecordingCoordinator
    private var cancellables = Set<AnyCancellable>()
    private var hasStarted = false

    init(model: AppModel, preferences: GlobalUtilityPreferencesStore) {
        self.preferences = preferences
        self.hotKeys = GlobalHotKeyService()
        self.screenshotCoordinator = ScreenshotCoordinator(model: model)
        self.quickSearchCoordinator = QuickSearchCoordinator(model: model)
        self.clipboardCoordinator = ClipboardHistoryCoordinator(model: model)
        self.screenRecordingCoordinator = ScreenRecordingCoordinator(model: model)
        self.quickSearchCoordinator.shortcutLabelProvider = { [weak self] in
            guard let self else { return "⌘ Space" }
            if case let .registered(activeHotKey, _) = self.hotKeys.states[.quickSearch] {
                return activeHotKey.displayName
            }
            return self.preferences.hotKey(for: .quickSearch).displayName
        }
        self.quickSearchCoordinator.onBuiltInAction = { [weak self, weak model] action in
            guard let self else { return }
            switch action {
            case .screenshot:
                self.startScreenshotAfterDismissingCommandPanels()
            case .screenRecording:
                self.perform(.screenRecording)
            case .clipboardHistory:
                self.perform(.clipboardHistory)
            case .openSettings:
                model?.openGlobalSearchSettings()
            case .openRuntimePermissions:
                model?.openGlobalSearchSettings(tab: .runtime)
            }
        }
    }

    func start() {
        guard !hasStarted else { return }
        hasStarted = true
        hotKeys.onAction = { [weak self] action in
            self?.perform(action)
        }
        preferences.$configurationRevision
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                guard let self else { return }
                self.hotKeys.reconfigure(preferences: self.preferences)
                self.clipboardCoordinator.setMonitoringEnabled(
                    self.preferences.isEnabled && self.preferences.clipboardEnabled
                )
            }
            .store(in: &cancellables)
        hotKeys.reconfigure(preferences: preferences)
        clipboardCoordinator.setMonitoringEnabled(preferences.isEnabled && preferences.clipboardEnabled)
    }

    func stop() {
        hotKeys.stop()
        clipboardCoordinator.setMonitoringEnabled(false)
        cancellables.removeAll()
        hasStarted = false
    }

    func trigger(_ action: GlobalUtilityAction) {
        perform(action)
    }

    private func perform(_ action: GlobalUtilityAction) {
        switch action {
        case .screenshot:
            startScreenshotAfterDismissingCommandPanels()
        case .quickSearch:
            quickSearchCoordinator.toggle()
        case .clipboardHistory:
            clipboardCoordinator.toggle()
        case .screenRecording:
            screenRecordingCoordinator.toggle()
        }
    }

    private func startScreenshotAfterDismissingCommandPanels() {
        let dismissedQuickSearch = quickSearchCoordinator.dismissIfPresented()
        let dismissedClipboard = clipboardCoordinator.dismissIfPresented()
        let dismissedRecordingPicker = screenRecordingCoordinator.dismissPickerIfPresented()
        let dismissedPanel = dismissedQuickSearch || dismissedClipboard || dismissedRecordingPicker

        guard dismissedPanel else {
            screenshotCoordinator.start()
            return
        }

        // Restoring the previously active application is asynchronous at the
        // WindowServer boundary. Let it become frontmost before creating the
        // screen-selection windows so no command panel can remain above them.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [weak self] in
            self?.screenshotCoordinator.start()
        }
    }
}
