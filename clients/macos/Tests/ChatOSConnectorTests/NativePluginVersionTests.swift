import ChatOSCore
import Testing
@testable import ChatOSConnector

struct NativePluginVersionTests {
    @Test
    func pluginUpdateStateUsesInstalledReleaseVersionAndArtifact() {
        let installed = NativeInstalledPluginRecord(
            pluginID: "plugin-a",
            releaseID: "release-1",
            version: "0.1.2",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: "/tmp/plugin-a/0.1.2",
            installedAt: "2026-09-04T00:00:00Z"
        )
        #expect(!NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "release-1",
                version: "0.1.2",
                artifactSHA256: String(repeating: "a", count: 64),
                npmPackage: nil
            )
        ))
        #expect(!NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "release-republished",
                version: "0.1.2",
                artifactSHA256: String(repeating: "a", count: 64),
                npmPackage: nil
            )
        ))
        #expect(NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "release-2",
                version: "0.1.3",
                artifactSHA256: String(repeating: "b", count: 64),
                npmPackage: nil
            )
        ))
        #expect(!NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "release-missing-version",
                version: nil,
                artifactSHA256: String(repeating: "b", count: 64),
                npmPackage: nil
            )
        ))
    }

    @Test
    func macOSPlatformGateRejectsWindowsOnlyPlugins() {
        #expect(NativePluginManifestLoader.macOSCompatible([]))
        #expect(NativePluginManifestLoader.macOSCompatible(["macos"]))
        #if arch(arm64)
        #expect(NativePluginManifestLoader.macOSCompatible(["macos-arm64"]))
        #elseif arch(x86_64)
        #expect(NativePluginManifestLoader.macOSCompatible(["macos-x64"]))
        #endif
        #expect(!NativePluginManifestLoader.macOSCompatible(["windows-x64", "windows-arm64"]))
    }

    @Test
    func webDesignStudioUpdateFrom0100To0110IsAvailable() {
        let installed = NativeInstalledPluginRecord(
            pluginID: "14993149-32e7-40a8-b166-dc7639edebfc",
            releaseID: "9a8dbb08-bb45-434e-a59e-40550867e7fb",
            version: "0.10.0",
            artifactSHA256: "e22f2a4e5b14d3e434fa4b01541d74dbcef31de0fc2586083962549467c0e20a",
            installationPath: "/tmp/chatos-web-design-studio/0.10.0",
            installedAt: "2026-09-07T00:00:00Z"
        )

        #expect(NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "c8a8a26c-4605-4477-acb0-9a84a04b3098",
                version: "0.11.0",
                artifactSHA256: "75de6a325efb6bc1c3034ba9e207461d7dcfb6945498fa3c08d27e82d4a673c3",
                npmPackage: nil
            )
        ))
    }
}
