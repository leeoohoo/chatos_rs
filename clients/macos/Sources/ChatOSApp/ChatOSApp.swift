import SwiftUI

@main
struct ChatOSApp: App {
    @NSApplicationDelegateAdaptor(ChatOSApplicationDelegate.self) private var appDelegate

    var body: some Scene {
        Settings {
            ChatOSSettingsSceneView(model: appDelegate.model)
                .frame(minWidth: 1_050, minHeight: 700)
        }
        .commands {
            ChatOSGlobalUtilityCommands(model: appDelegate.model)
        }
    }
}

struct ChatOSMainSceneView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        RootView()
            .environmentObject(model)
            .environment(\.locale, model.interfaceLocale)
            .environment(\.interfaceFontScale, model.interfaceFontScale)
            .dynamicTypeSize(model.interfaceDynamicTypeSize)
            .frame(minWidth: 1_100, minHeight: 720)
    }
}

private struct ChatOSSettingsSceneView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        SettingsView()
            .environmentObject(model)
            .environment(\.locale, model.interfaceLocale)
            .environment(\.interfaceFontScale, model.interfaceFontScale)
            .dynamicTypeSize(model.interfaceDynamicTypeSize)
    }
}

private struct ChatOSGlobalUtilityCommands: Commands {
    @ObservedObject var model: AppModel

    var body: some Commands {
        CommandMenu(model.localized("全局工具", english: "Global Utilities")) {
            Button(model.localized("区域截图", english: "Region Screenshot")) {
                model.globalUtilityCoordinator.trigger(.screenshot)
            }
            Divider()
            Button(model.localized("剪贴板历史", english: "Clipboard History")) {
                model.globalUtilityCoordinator.trigger(.clipboardHistory)
            }
            Button(model.localized("快速搜索", english: "Quick Search")) {
                model.globalUtilityCoordinator.trigger(.quickSearch)
            }
        }
    }
}
