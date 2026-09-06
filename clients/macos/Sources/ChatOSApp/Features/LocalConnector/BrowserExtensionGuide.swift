import AppKit
import ChatOSCore
import Foundation

enum BrowserExtensionGuide {
    static let browserPackageName = "chatos-browser-cdp"
    static let extensionID = "jooaepjckiofmpldinopgdgddcoaofil"
    static let webStoreURL = URL(
        string: "https://chromewebstore.google.com/detail/\(extensionID)"
    )!
    static let onboardingURL = URL(
        string: "chrome-extension://\(extensionID)/onboarding/onboarding.html"
    )!

    private static let chromeBundleIdentifiers = [
        "com.google.Chrome",
        "com.google.Chrome.beta",
        "com.google.Chrome.canary",
    ]

    static func isBrowserPlugin(_ plugin: LocalConnectorPlugin) -> Bool {
        let packageFromPluginKey = plugin.pluginKey?.split(
            separator: "@",
            maxSplits: 1,
            omittingEmptySubsequences: true
        ).first.map(String.init)
        return [plugin.packageName, packageFromPluginKey, plugin.pluginID]
            .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines) }
            .contains(browserPackageName)
    }

    static func isExtensionInstalled(fileManager: FileManager = .default) -> Bool {
        let applicationSupport = fileManager.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support", isDirectory: true)
        let userDataRoots = [
            applicationSupport.appendingPathComponent("Google/Chrome", isDirectory: true),
            applicationSupport.appendingPathComponent("Google/Chrome Beta", isDirectory: true),
            applicationSupport.appendingPathComponent("Google/Chrome Canary", isDirectory: true),
        ]

        return userDataRoots.contains { root in
            guard let profiles = try? fileManager.contentsOfDirectory(
                at: root,
                includingPropertiesForKeys: [.isDirectoryKey],
                options: [.skipsHiddenFiles]
            ) else {
                return false
            }
            return profiles.contains { profile in
                let extensionRoot = profile
                    .appendingPathComponent("Extensions", isDirectory: true)
                    .appendingPathComponent(extensionID, isDirectory: true)
                guard let versions = try? fileManager.contentsOfDirectory(
                    at: extensionRoot,
                    includingPropertiesForKeys: [.isDirectoryKey],
                    options: [.skipsHiddenFiles]
                ) else {
                    return false
                }
                return !versions.isEmpty
            }
        }
    }

    @MainActor
    static func openWebStore() {
        openInChrome(webStoreURL)
    }

    @MainActor
    static func openOnboarding() {
        openInChrome(onboardingURL)
    }

    @MainActor
    private static func openInChrome(_ url: URL) {
        if let chromeApplicationURL = chromeBundleIdentifiers.lazy.compactMap({
            NSWorkspace.shared.urlForApplication(withBundleIdentifier: $0)
        }).first {
            NSWorkspace.shared.open(
                [url],
                withApplicationAt: chromeApplicationURL,
                configuration: NSWorkspace.OpenConfiguration()
            )
        } else {
            NSWorkspace.shared.open(url)
        }
    }

    @MainActor
    static func openWebStoreAfterInstallIfNeeded(
        pluginVersion: String,
        defaults: UserDefaults = .standard
    ) {
        guard !isExtensionInstalled() else { return }
        defaults.set(true, forKey: promptKey(pluginVersion: pluginVersion))
        openWebStore()
    }

    @MainActor
    static func automaticallyGuideIfNeeded(
        pluginVersion: String,
        defaults: UserDefaults = .standard
    ) {
        guard !isExtensionInstalled() else { return }
        let promptKey = promptKey(pluginVersion: pluginVersion)
        guard !defaults.bool(forKey: promptKey) else { return }
        defaults.set(true, forKey: promptKey)
        openWebStore()
    }

    static func shouldAutomaticallyGuide(
        pluginVersion: String,
        defaults: UserDefaults = .standard
    ) -> Bool {
        !isExtensionInstalled()
            && !defaults.bool(forKey: promptKey(pluginVersion: pluginVersion))
    }

    private static func promptKey(pluginVersion: String) -> String {
        let appVersion = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
            ?? "development"
        return "ChatOS.browserExtensionGuide.\(appVersion).\(pluginVersion)"
    }
}
