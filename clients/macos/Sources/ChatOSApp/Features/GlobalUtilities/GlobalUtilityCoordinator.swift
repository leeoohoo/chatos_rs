import ChatOSCore
import Combine
import Foundation

@MainActor
final class GlobalUtilityCoordinator {
    let preferences: GlobalUtilityPreferencesStore
    let hotKeys: GlobalHotKeyService

    private let placeholderPanel: GlobalUtilityPlaceholderPanelController
    private let screenshotCoordinator: ScreenshotCoordinator
    private var cancellables = Set<AnyCancellable>()
    private var hasStarted = false

    init(model: AppModel, preferences: GlobalUtilityPreferencesStore) {
        self.preferences = preferences
        self.hotKeys = GlobalHotKeyService()
        self.placeholderPanel = GlobalUtilityPlaceholderPanelController(model: model)
        self.screenshotCoordinator = ScreenshotCoordinator(model: model)
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
            }
            .store(in: &cancellables)
        hotKeys.reconfigure(preferences: preferences)
    }

    func stop() {
        hotKeys.stop()
        cancellables.removeAll()
        hasStarted = false
    }

    private func perform(_ action: GlobalUtilityAction) {
        switch action {
        case .screenshot:
            screenshotCoordinator.start()
        case .screenRecording, .clipboardHistory, .quickSearch:
            placeholderPanel.toggle(action: action)
        }
    }
}
