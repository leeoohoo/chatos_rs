import AppKit
import SwiftUI

@MainActor
final class ClipboardHistoryCoordinator {
    private weak var model: AppModel?
    private let store: ClipboardHistoryStore
    private let monitor: ClipboardHistoryMonitor
    private let viewModel: ClipboardHistoryViewModel
    private let panelController: GlobalCommandPanelController

    init(model: AppModel) {
        self.model = model
        let store = ClipboardHistoryStore()
        self.store = store
        self.monitor = ClipboardHistoryMonitor(store: store)
        self.viewModel = ClipboardHistoryViewModel(store: store)
        self.panelController = GlobalCommandPanelController(size: NSSize(width: 700, height: 520))

        monitor.onEntryStored = { [weak viewModel] entry in
            viewModel?.entryWasStored(entry)
        }
        viewModel.onRestoreSucceeded = { [weak panelController] in
            panelController?.closeAndRestorePreviousApplication()
        }
        viewModel.onCancel = { [weak panelController] in
            panelController?.closeAndRestorePreviousApplication()
        }
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
        panelController.present()
    }
}
