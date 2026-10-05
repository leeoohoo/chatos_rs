import Foundation

enum NativePluginProcessEnvironment {
    private static let inheritedKeys: Set<String> = [
        "LANG", "LC_ALL", "LC_CTYPE", "TMPDIR",
    ]

    static func make(
        base: [String: String] = ProcessInfo.processInfo.environment,
        overrides: [String: String] = [:]
    ) -> [String: String] {
        var environment = base.filter { inheritedKeys.contains($0.key) }
        environment["PATH"] = runtimePath()
        environment.merge(
            overrides.filter { $0.key != "PATH" },
            uniquingKeysWith: { _, runtime in runtime }
        )
        if let dataDirectory = overrides["CHATOS_PLUGIN_DATA_DIR"] {
            environment["HOME"] = dataDirectory
        }
        return environment
    }

    static func runtimePath() -> String {
        "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"
    }
}
