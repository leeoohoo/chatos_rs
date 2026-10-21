import AppKit
import ChatOSCore
import SwiftUI

@MainActor
final class QuickSearchCoordinator {
    var onBuiltInAction: ((QuickSearchBuiltInAction) -> Void)?
    var shortcutLabelProvider: (() -> String)?

    private weak var context: (any GlobalUtilityContext)?
    private let viewModel: QuickSearchViewModel
    private let panelController: GlobalCommandPanelController

    init(context: any GlobalUtilityContext) {
        self.context = context
        self.viewModel = QuickSearchViewModel(context: context)
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
            isEnglish: context?.globalUtilityIsEnglish == true,
            shortcutLabel: shortcutLabelProvider?() ?? "⌘ Space"
        ))
        panelController.present(focusingFirstTextInput: true)
    }

    @discardableResult
    func dismissIfPresented() -> Bool {
        guard panelController.isPresented else { return false }
        panelController.closeAndRestorePreviousApplication()
        return true
    }

    private func execute(_ action: QuickSearchAction) {
        switch action {
        case let .openProject(projectID):
            panelController.closeWithoutRestoringPreviousApplication()
            context?.globalUtilityOpenProject(projectID)
        case let .openContact(contactID):
            panelController.closeWithoutRestoringPreviousApplication()
            context?.globalUtilityOpenContact(contactID)
        case let .openApplication(url):
            panelController.closeWithoutRestoringPreviousApplication()
            let configuration = NSWorkspace.OpenConfiguration()
            configuration.activates = true
            NSWorkspace.shared.openApplication(at: url, configuration: configuration)
        case let .openFile(url):
            panelController.closeWithoutRestoringPreviousApplication()
            NSWorkspace.shared.open(url)
        case let .revealFile(url):
            panelController.closeWithoutRestoringPreviousApplication()
            NSWorkspace.shared.activateFileViewerSelecting([url])
        case let .builtIn(action):
            switch action {
            case .screenshot, .screenRecording, .clipboardHistory:
                panelController.closeAndRestorePreviousApplication { [weak self] in
                    self?.onBuiltInAction?(action)
                }
            case .openSettings, .openRuntimePermissions:
                panelController.closeWithoutRestoringPreviousApplication()
                onBuiltInAction?(action)
            }
        }
    }
}
