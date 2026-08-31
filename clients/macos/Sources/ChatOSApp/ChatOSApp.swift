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
                .task { model.startPetOverlayIfNeeded() }
        }
        .defaultSize(width: 1_440, height: 900)
        .windowStyle(.titleBar)

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
