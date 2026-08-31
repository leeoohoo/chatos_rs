import AppKit
import ChatOSCore
import SwiftUI

@MainActor
final class GlobalUtilityPlaceholderPanelController: GlobalCommandPanelController {
    private var presentedAction: GlobalUtilityAction?
    private weak var model: AppModel?

    init(model: AppModel) {
        self.model = model
        super.init(size: NSSize(width: 560, height: 310))
    }

    func toggle(action: GlobalUtilityAction) {
        if isPresented, presentedAction == action {
            closeAndRestorePreviousApplication()
            return
        }
        presentedAction = action
        setRootView(GlobalUtilityPlaceholderView(
            action: action,
            isEnglish: model?.interfaceLanguage == .english
        ))
        present()
    }
}

private struct GlobalUtilityPlaceholderView: View {
    var action: GlobalUtilityAction
    var isEnglish: Bool

    var body: some View {
        VStack(spacing: 18) {
            Image(systemName: icon)
                .font(.system(size: 42, weight: .semibold))
                .foregroundStyle(.tint)
            Text(title)
                .appFont(.title2.weight(.semibold))
            Text(detail)
                .appFont(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 420)
            Text(localized("按 Escape 关闭", "Press Escape to close"))
                .appFont(.caption)
                .foregroundStyle(.tertiary)
        }
        .padding(34)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.ultraThickMaterial, in: RoundedRectangle(cornerRadius: 22))
        .overlay {
            RoundedRectangle(cornerRadius: 22)
                .strokeBorder(.white.opacity(0.18), lineWidth: 1)
        }
    }

    private var title: String {
        switch action {
        case .screenshot: localized("截屏", "Screenshot")
        case .screenRecording: localized("录屏", "Screen Recording")
        case .clipboardHistory: localized("剪贴板历史", "Clipboard History")
        case .quickSearch: localized("ChatOS 快速搜索", "ChatOS Quick Search")
        }
    }

    private var detail: String {
        localized(
            "全局快捷键已经生效。这个入口正在接入完整功能。",
            "The global shortcut is active. The complete workflow is being connected here."
        )
    }

    private var icon: String {
        switch action {
        case .screenshot: "viewfinder"
        case .screenRecording: "record.circle"
        case .clipboardHistory: "clipboard"
        case .quickSearch: "magnifyingglass"
        }
    }

    private func localized(_ chinese: String, _ english: String) -> String {
        isEnglish ? english : chinese
    }
}
