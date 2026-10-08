import Foundation

enum NativePluginProcessEnvironment {
    private static let inheritedKeys: Set<String> = [
        "LANG", "LC_ALL", "LC_CTYPE", "TMPDIR",
    ]

    static func make(
        base: [String: String] = ProcessInfo.processInfo.environment,
        overrides: [String: String] = [:],
        homeDirectory: String? = nil
    ) -> [String: String] {
        var environment = base.filter { inheritedKeys.contains($0.key) }
        environment["PATH"] = runtimePath()
        environment.merge(
            overrides.filter { $0.key != "PATH" },
            uniquingKeysWith: { _, runtime in runtime }
        )
        if let homeDirectory {
            environment["HOME"] = homeDirectory
        } else if let dataDirectory = overrides["CHATOS_PLUGIN_DATA_DIR"] {
            environment["HOME"] = dataDirectory
        }
        // A Plugin must never fall back to the desktop user's shared temporary directory.
        // Document rendering and Web Design screenshots legitimately need temporary files,
        // so bind their standard temporary-directory APIs to the already isolated cache root.
        if let cacheDirectory = overrides["CHATOS_PLUGIN_CACHE_DIR"] {
            environment["TMPDIR"] = cacheDirectory
        } else {
            environment.removeValue(forKey: "TMPDIR")
        }
        return environment
    }

    static func runtimePath() -> String {
        "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"
    }
}
