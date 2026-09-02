import SwiftUI

@main
struct ChatOSApp: App {
    @StateObject private var model = AppModel()

    var body: some Scene {
        WindowGroup("ChatOS") {
            RootView()
                .environmentObject(model)
                .environment(\.locale, model.interfaceLocale)
                .environment(\.interfaceFontScale, model.interfaceFontScale)
                .dynamicTypeSize(model.interfaceDynamicTypeSize)
                .frame(minWidth: 1_100, minHeight: 720)
                .task {
                    model.startPetOverlayIfNeeded()
                    model.startGlobalUtilitiesIfNeeded()
                }
        }
        .defaultSize(width: 1_440, height: 900)
        .windowStyle(.titleBar)
        .commands {
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

        Settings {
            SettingsView()
                .environmentObject(model)
                .environment(\.locale, model.interfaceLocale)
                .environment(\.interfaceFontScale, model.interfaceFontScale)
                .dynamicTypeSize(model.interfaceDynamicTypeSize)
                .frame(minWidth: 1_050, minHeight: 700)
        }
    }
}
