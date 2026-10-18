import Foundation

actor NativePluginIntegritySnapshotCache {
    private struct Key: Hashable {
        let pluginID: String
        let releaseID: String
        let artifactSHA256: String
        let installationPath: String

        init(record: NativeInstalledPluginRecord) {
            pluginID = record.pluginID
            releaseID = record.releaseID
            artifactSHA256 = record.artifactSHA256
            installationPath = record.installationPath
        }
    }

    private var statuses: [Key: NativePluginInstallationIntegrity.Status] = [:]

    func status(
        record: NativeInstalledPluginRecord,
        forceRefresh: Bool = false
    ) async -> NativePluginInstallationIntegrity.Status {
        let key = Key(record: record)
        if !forceRefresh, let status = statuses[key] { return status }
        let status = await Task.detached(priority: .utility) {
            NativePluginInstallationIntegrity.status(record: record)
        }.value
        statuses[key] = status
        return status
    }

    func invalidate(record: NativeInstalledPluginRecord) {
        statuses[Key(record: record)] = nil
    }

    func invalidateAll() {
        statuses.removeAll(keepingCapacity: true)
    }
}
