import AppKit
import ChatOSConnector
import Combine
import SwiftUI

@MainActor
final class ClipboardHistoryCoordinator {
    private weak var model: AppModel?
    private let store: ClipboardHistoryStore
    private let monitor: ClipboardHistoryMonitor
    private let viewModel: ClipboardHistoryViewModel
    private let panelController: GlobalCommandPanelController
    private var cancellables = Set<AnyCancellable>()

    init(model: AppModel) {
        self.model = model
        let store = ClipboardHistoryStore()
        self.store = store
        self.monitor = ClipboardHistoryMonitor(store: store)
        self.viewModel = ClipboardHistoryViewModel(store: store)
        self.panelController = GlobalCommandPanelController(size: NSSize(width: 700, height: 340))

        monitor.onEntryStored = { [weak viewModel] entry in
            viewModel?.entryWasStored(entry)
        }
        viewModel.onRestoreSucceeded = { [weak panelController, weak viewModel] in
            guard NativePasteService.hasAccessibilityAccess else {
                NativePasteService.requestAccessibilityAccess()
                viewModel?.showAutomaticPastePermissionNotice()
                return
            }
            panelController?.closeAndRestorePreviousApplication {
                NativePasteService.postPasteShortcut()
            }
        }
        viewModel.onCancel = { [weak panelController] in
            panelController?.closeAndRestorePreviousApplication()
        }
        viewModel.$entries
            .combineLatest(viewModel.$isLoading)
            .sink { [weak panelController] entries, _ in
                let height: CGFloat = entries.isEmpty ? 340 : 520
                panelController?.setContentSize(NSSize(width: 700, height: height))
            }
            .store(in: &cancellables)
    }

    func setMonitoringEnabled(_ enabled: Bool) {
        enabled ? monitor.start() : monitor.stop()
    }

    func toggle() {
        if panelController.isPresented {
            panelController.closeAndRestorePreviousApplication()
            return
        }
        viewModel.prepareForPresentation()
        panelController.setRootView(ClipboardHistoryView(
            viewModel: viewModel,
            isEnglish: model?.interfaceLanguage == .english
        ))
        panelController.present(focusingFirstTextInput: true)
    }

    @discardableResult
    func dismissIfPresented() -> Bool {
        guard panelController.isPresented else { return false }
        panelController.closeAndRestorePreviousApplication()
        return true
    }
}
