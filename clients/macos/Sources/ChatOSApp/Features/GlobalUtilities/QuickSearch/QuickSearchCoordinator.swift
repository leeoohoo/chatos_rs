import AppKit
import ChatOSCore
import SwiftUI

@MainActor
final class QuickSearchCoordinator {
    var onBuiltInAction: ((QuickSearchBuiltInAction) -> Void)?
    var shortcutLabelProvider: (() -> String)?

    private weak var model: AppModel?
    private let viewModel: QuickSearchViewModel
    private let panelController: GlobalCommandPanelController

    init(model: AppModel) {
        self.model = model
        self.viewModel = QuickSearchViewModel(model: model)
        self.panelController = GlobalCommandPanelController(size: NSSize(width: 760, height: 520))
        viewModel.onExecute = { [weak self] action in
            self?.execute(action)
        }
        viewModel.onCancel = { [weak self] in
            self?.panelController.closeAndRestorePreviousApplication()
        }
    }

    func toggle() {
        if panelController.isPresented {
            panelController.closeAndRestorePreviousApplication()
            return
        }
        viewModel.prepareForPresentation()
        panelController.setRootView(QuickSearchView(
            viewModel: viewModel,
            isEnglish: model?.interfaceLanguage == .english,
            shortcutLabel: shortcutLabelProvider?() ?? "⌘ Space"
        ))
        panelController.present()
    }

    private func execute(_ action: QuickSearchAction) {
        panelController.closeWithoutRestoringPreviousApplication()
        switch action {
        case let .openProject(projectID):
            model?.openGlobalSearchProject(projectID)
        case let .openContact(contactID):
            model?.openGlobalSearchContact(contactID)
        case let .openApplication(url):
            let configuration = NSWorkspace.OpenConfiguration()
            configuration.activates = true
            NSWorkspace.shared.openApplication(at: url, configuration: configuration)
        case let .openFile(url):
            NSWorkspace.shared.open(url)
        case let .revealFile(url):
            NSWorkspace.shared.activateFileViewerSelecting([url])
        case let .builtIn(action):
            onBuiltInAction?(action)
        }
    }
}
