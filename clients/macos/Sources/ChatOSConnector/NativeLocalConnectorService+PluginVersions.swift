import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    static func pluginUpdateAvailable(
        installed: NativeInstalledPluginRecord?,
        release: GatewayPluginReleaseDTO
    ) -> Bool {
        guard let installed else { return false }
        guard installed.releaseID != release.id else { return false }
        guard let latestVersion = normalizedPluginVersion(release.version) else { return false }
        let latestArtifact = release.artifactSHA256?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        return normalizedPluginVersion(installed.version) != latestVersion
            || latestArtifact.map { installed.artifactSHA256 != $0 } == true
    }

    private static func normalizedPluginVersion(_ value: String?) -> String? {
        let normalized = value?.trimmingCharacters(in: .whitespacesAndNewlines)
        return normalized?.isEmpty == false ? normalized : nil
    }
}
