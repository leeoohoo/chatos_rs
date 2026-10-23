import AppKit
import ChatOSCore
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
        // ChatOS is also the owner of the pet overlay, global utilities, and the
        // Agent Host. AppKit must not automatically terminate it during the
        // short launch interval before the manually managed main window appears,
        // nor when that window is closed while background features remain active.
        ProcessInfo.processInfo.disableAutomaticTermination(
            "ChatOS background services remain active without a main window"
        )
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
        if Self.isBuildSmokeTest {
            do {
                try BundledAgentSkillLoader.validateCatalog()
            } catch {
                fatalError("Packaged ChatOS resource validation failed: \(error.localizedDescription)")
            }
        }

        NSApp.servicesProvider = self
        NSUpdateDynamicServices()
        model.startPetOverlayIfNeeded()
        model.startGlobalUtilitiesIfNeeded()

        if let smokeExitDelay = Self.buildSmokeTestExitDelay {
            DispatchQueue.main.asyncAfter(deadline: .now() + smokeExitDelay) {
                NSApp.terminate(nil)
            }
        }

        // A document-open Apple event may arrive before didFinishLaunching.
        // In that launch path, never schedule creation of the main window.
        if !didReceiveFileOpenRequest {
            scheduleMainWindowPresentation(after: 0.2)
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        model.prepareForApplicationTermination()
    }

    private static var buildSmokeTestExitDelay: TimeInterval? {
        let environment = ProcessInfo.processInfo.environment
        guard isBuildSmokeTest,
              let rawValue = environment["CHATOS_BUILD_SMOKE_TEST_EXIT_AFTER_SECONDS"],
              let delay = TimeInterval(rawValue),
              delay > 0 else { return nil }
        return delay
    }

    private static var isBuildSmokeTest: Bool {
        ProcessInfo.processInfo.environment["CHATOS_BUILD_SMOKE_TEST"] == "1"
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
        // Pet and running-task panels can remain visible after the main window
        // is closed. In that state AppKit considers the application to have
        // visible windows, but a Dock click still means “show the app”. Reopen
        // synchronously so an auxiliary panel cannot absorb the activation and
        // leave the user with no route back to the main workspace.
        showMainWindow()
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
        guard let window = controller.window else { return }
        Self.restoreToVisibleScreenIfNeeded(window)
        if window.isMiniaturized {
            window.deminiaturize(nil)
        }
        // The main window must remain a primary full-screen window. Marking it
        // as `canJoinAllSpaces`/`fullScreenAuxiliary` makes AppKit treat it like
        // a floating companion and disables its own native full-screen Space.
        window.collectionBehavior.remove([.canJoinAllSpaces, .fullScreenAuxiliary])
        window.collectionBehavior.insert([.moveToActiveSpace, .fullScreenPrimary])
        NSApp.activate(ignoringOtherApps: true)
        controller.showWindow(nil)
        window.makeKeyAndOrderFront(nil)
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
        window.titleVisibility = .hidden
        window.minSize = NSSize(width: 1_100, height: 720)
        window.toolbarStyle = .unifiedCompact
        // Keep ordinary window presentations on the active Space while still
        // allowing the main window to create and own a native full-screen Space.
        window.collectionBehavior.insert([.moveToActiveSpace, .fullScreenPrimary])
        window.contentViewController = hostingController
        window.isReleasedWhenClosed = false
        installTitlebarActions(in: window)
        window.setFrameAutosaveName("ChatOSMainWindow")
        if !window.setFrameUsingName("ChatOSMainWindow") {
            window.center()
        } else if !Self.isMeaningfullyVisible(window.frame) {
            window.center()
        }
        return NSWindowController(window: window)
    }

    private static func isMeaningfullyVisible(_ frame: NSRect) -> Bool {
        NSScreen.screens.contains { screen in
            let intersection = frame.intersection(screen.visibleFrame)
            return intersection.width >= 160 && intersection.height >= 120
        }
    }

    private static func restoreToVisibleScreenIfNeeded(_ window: NSWindow) {
        guard !isMeaningfullyVisible(window.frame),
              let screen = NSScreen.main ?? NSScreen.screens.first else { return }
        let visibleFrame = screen.visibleFrame
        var frame = window.frame
        frame.size.width = min(frame.width, visibleFrame.width)
        frame.size.height = min(frame.height, visibleFrame.height)
        frame.origin.x = visibleFrame.midX - frame.width / 2
        frame.origin.y = visibleFrame.midY - frame.height / 2
        window.setFrame(frame, display: false)
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
            contentRect: NSRect(x: 0, y: 0, width: 1_280, height: 840),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = model.localized("设置", english: "Settings")
        window.titleVisibility = .hidden
        window.minSize = NSSize(width: 1_180, height: 760)
        window.toolbarStyle = .unifiedCompact
        window.contentViewController = hostingController
        window.isReleasedWhenClosed = false
        window.tabbingMode = .disallowed
        window.setFrameAutosaveName("ChatOSSettingsWindow")
        if !window.setFrameUsingName("ChatOSSettingsWindow") {
            window.center()
        } else if window.frame.width < window.minSize.width
            || window.frame.height < window.minSize.height {
            var frame = window.frame
            let topEdge = frame.maxY
            frame.size.width = max(frame.width, window.minSize.width)
            frame.size.height = max(frame.height, window.minSize.height)
            frame.origin.y = topEdge - frame.height
            window.setFrame(frame, display: false)
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
