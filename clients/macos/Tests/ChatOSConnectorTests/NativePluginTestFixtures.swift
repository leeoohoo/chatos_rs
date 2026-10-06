@testable import ChatOSConnector
import Foundation

func nativePluginTestRecord(
    installationURL: URL,
    pluginID: String = "plugin-fixture",
    releaseID: String = "release-fixture",
    version: String = "1.0.0"
) throws -> NativeInstalledPluginRecord {
    .init(
        pluginID: pluginID,
        releaseID: releaseID,
        version: version,
        artifactSHA256: String(repeating: "a", count: 64),
        installationPath: installationURL.path,
        installedAt: "2026-10-06T00:00:00Z",
        packageFileSHA256: try NativePluginInstallationIntegrity.snapshot(
            installationURL: installationURL,
            maximumFiles: 20_000,
            maximumBytes: 512 * 1_024 * 1_024
        )
    )
}
