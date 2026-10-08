import Foundation

enum NativePluginApplicationHostContract {
    static let runtimeType = "local_http"

    static func environment(
        record: NativeInstalledPluginRecord,
        componentKey: String,
        installationURL: URL,
        dataURL: URL,
        cacheURL: URL,
        port: UInt16
    ) -> [String: String] {
        [
            "CHATOS_PLUGIN_ROOT": installationURL.path,
            "CHATOS_PLUGIN_DATA_DIR": dataURL.path,
            "CHATOS_PLUGIN_CACHE_DIR": cacheURL.path,
            "CHATOS_PLUGIN_APP_HOST": "127.0.0.1",
            "CHATOS_PLUGIN_APP_PORT": String(port),
            "CHATOS_PLUGIN_ID": record.pluginID,
            "CHATOS_PLUGIN_COMPONENT_KEY": componentKey,
            "CHATOS_PLUGIN_RELEASE_ID": record.releaseID,
            "CHATOS_PLUGIN_VERSION": record.version,
            "CHATOS_PLUGIN_ARTIFACT_SHA256": record.artifactSHA256,
            // Node's native directory watcher cannot initialize under the
            // macOS Seatbelt profile. Applications consume this generic Host
            // contract through the shared local_http runtime and use bounded
            // polling instead of carrying plugin-specific workarounds.
            "CHATOS_PLUGIN_FILE_WATCH_MODE": "polling",
        ]
    }

    static func writableSidecarURLs(dataURL: URL, cacheURL: URL) -> [URL] {
        [dataURL, cacheURL].map {
            URL(fileURLWithPath: $0.path + ".lock", isDirectory: true)
        }
    }
}
