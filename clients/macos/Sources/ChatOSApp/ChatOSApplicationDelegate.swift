import AppKit
import Foundation
import SwiftUI

@MainActor
final class ChatOSApplicationDelegate: NSObject, NSApplicationDelegate {
    let model: AppModel

    private var mainWindowController: NSWindowController?
    private var settingsWindowController: NSWindowController?
    private var mainWindowPresentationGeneration = 0
    private var didReceiveFileOpenRequest = false

    override init() {
        let model = AppModel()
        self.model = model
        super.init()
        model.mainWindowPresentationHandler = { [weak self] in
            self?.showMainWindow()
        }
        model.settingsWindowPresentationHandler = { [weak self] in
            self?.showSettingsWindow()
        }
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.servicesProvider = self
        NSUpdateDynamicServices()
        model.startPetOverlayIfNeeded()
        model.startGlobalUtilitiesIfNeeded()

        // A document-open Apple event may arrive before didFinishLaunching.
        // In that launch path, never schedule creation of the main window.
        if !didReceiveFileOpenRequest {
            scheduleMainWindowPresentation(after: 0.2)
        }
    }

    func application(_ sender: NSApplication, openFiles filenames: [String]) {
        receive(
            filenames.map { URL(fileURLWithPath: $0) },
            offerDefaultHandlerPrompt: true
        )
        sender.reply(toOpenOrPrint: .success)
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        receive(urls, offerDefaultHandlerPrompt: true)
    }

    func applicationShouldHandleReopen(
        _ sender: NSApplication,
        hasVisibleWindows flag: Bool
    ) -> Bool {
        scheduleMainWindowPresentation(after: 0.08)
        return false
    }

    @objc func openFilesInPet(
        _ pasteboard: NSPasteboard,
        userData: String,
        error: AutoreleasingUnsafeMutablePointer<NSString?>
    ) {
        let options: [NSPasteboard.ReadingOptionKey: Any] = [
            .urlReadingFileURLsOnly: true,
        ]
        let urls = pasteboard.readObjects(
            forClasses: [NSURL.self],
            options: options
        ) as? [URL] ?? []
        guard !urls.isEmpty else {
            error.pointee = "没有收到可打开的文件"
            return
        }
        receive(urls, offerDefaultHandlerPrompt: false)
    }

    private func receive(_ urls: [URL], offerDefaultHandlerPrompt: Bool) {
        let files = urls
            .filter(\.isFileURL)
            .map(\.standardizedFileURL)
        guard !files.isEmpty else { return }
        didReceiveFileOpenRequest = true
        cancelPendingMainWindowPresentation()
        openInPet(files)
        if offerDefaultHandlerPrompt {
            model.petDefaultFileHandlerPrompt.offerAfterOpening(files)
        }

        // Launch Services may activate ChatOS to deliver the open event. The
        // pet file desk uses a non-activating panel, so return focus immediately.
        DispatchQueue.main.async {
            NSApp.deactivate()
        }
    }

    private func openInPet(_ urls: [URL]) {
        model.openUserSelectedPetFiles(urls)
    }

    private func scheduleMainWindowPresentation(after delay: TimeInterval) {
        mainWindowPresentationGeneration += 1
        let generation = mainWindowPresentationGeneration
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
            guard let self,
                  generation == self.mainWindowPresentationGeneration else { return }
            self.showMainWindow()
        }
    }

    private func cancelPendingMainWindowPresentation() {
        mainWindowPresentationGeneration += 1
    }

    private func showMainWindow() {
        cancelPendingMainWindowPresentation()
        let controller = mainWindowController ?? makeMainWindowController()
        mainWindowController = controller
        NSApp.activate(ignoringOtherApps: true)
        controller.showWindow(nil)
        controller.window?.makeKeyAndOrderFront(nil)
    }

    private func makeMainWindowController() -> NSWindowController {
        let content = ChatOSMainSceneView(model: model)
        let hostingController = NSHostingController(rootView: content)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1_440, height: 900),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "ChatOS"
        window.minSize = NSSize(width: 1_100, height: 720)
        window.contentViewController = hostingController
        window.isReleasedWhenClosed = false
        installTitlebarActions(in: window)
        window.setFrameAutosaveName("ChatOSMainWindow")
        if !window.setFrameUsingName("ChatOSMainWindow") {
            window.center()
        }
        return NSWindowController(window: window)
    }

    private func showSettingsWindow() {
        let controller = settingsWindowController ?? makeSettingsWindowController()
        settingsWindowController = controller
        NSApp.activate(ignoringOtherApps: true)
        controller.showWindow(nil)
        controller.window?.makeKeyAndOrderFront(nil)
    }

    private func makeSettingsWindowController() -> NSWindowController {
        let content = SettingsView()
            .environmentObject(model)
            .environment(\.locale, model.interfaceLocale)
            .environment(\.interfaceFontScale, model.interfaceFontScale)
            .dynamicTypeSize(model.interfaceDynamicTypeSize)
        let hostingController = NSHostingController(rootView: content)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1_050, height: 700),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = model.localized("设置", english: "Settings")
        window.minSize = NSSize(width: 900, height: 620)
        window.contentViewController = hostingController
        window.isReleasedWhenClosed = false
        window.tabbingMode = .disallowed
        window.setFrameAutosaveName("ChatOSSettingsWindow")
        if !window.setFrameUsingName("ChatOSSettingsWindow") {
            window.center()
        }
        return NSWindowController(window: window)
    }

    private func installTitlebarActions(in window: NSWindow) {
        let actions = WorkspaceTitlebarActionsView(model: model)
            .environment(\.locale, model.interfaceLocale)
            .environment(\.interfaceFontScale, model.interfaceFontScale)
            .dynamicTypeSize(model.interfaceDynamicTypeSize)
        let hostingView = NSHostingView(rootView: actions)
        hostingView.frame = NSRect(x: 0, y: 0, width: 96, height: 36)

        let accessory = NSTitlebarAccessoryViewController()
        accessory.layoutAttribute = .right
        accessory.view = hostingView
        window.addTitlebarAccessoryViewController(accessory)
    }
}
