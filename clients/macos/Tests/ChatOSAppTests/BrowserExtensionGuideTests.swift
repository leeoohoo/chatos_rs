import ChatOSCore
import Testing
@testable import ChatOSApp

struct BrowserExtensionGuideTests {
    @Test
    func recognizesBrowserPluginByMarketplacePackageIdentity() {
        let plugin = makePlugin(
            pluginID: "ed794425-0fe9-4eea-89a8-9e70de898706",
            packageName: "chatos-browser-cdp",
            pluginKey: "chatos-browser-cdp@chatos-marketplace"
        )

        #expect(BrowserExtensionGuide.isBrowserPlugin(plugin))
    }

    @Test
    func recognizesBrowserPluginByPluginKeyWhenPackageNameIsUnavailable() {
        let plugin = makePlugin(
            pluginID: "ed794425-0fe9-4eea-89a8-9e70de898706",
            packageName: nil,
            pluginKey: "chatos-browser-cdp@chatos-marketplace"
        )

        #expect(BrowserExtensionGuide.isBrowserPlugin(plugin))
    }

    @Test
    func doesNotTreatDisplayNameAsPluginIdentity() {
        let plugin = makePlugin(
            pluginID: "unrelated-plugin-id",
            packageName: "another-browser-plugin",
            pluginKey: "another-browser-plugin@chatos-marketplace"
        )

        #expect(!BrowserExtensionGuide.isBrowserPlugin(plugin))
    }

    private func makePlugin(
        pluginID: String,
        packageName: String?,
        pluginKey: String?
    ) -> LocalConnectorPlugin {
        LocalConnectorPlugin(
            pluginID: pluginID,
            packageName: packageName,
            pluginKey: pluginKey,
            displayName: "Browser CDP",
            description: "Browser control",
            category: "Developer Tools",
            publisher: "Chatos",
            latestVersion: "0.1.8",
            installed: true,
            updateAvailable: false,
            installAvailable: true,
            enabled: true
        )
    }
}
