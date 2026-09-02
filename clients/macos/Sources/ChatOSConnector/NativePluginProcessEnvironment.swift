import Foundation

enum NativePluginProcessEnvironment {
    static func make(
        base: [String: String] = ProcessInfo.processInfo.environment,
        overrides: [String: String] = [:]
    ) -> [String: String] {
        var environment = base
        environment.merge(overrides, uniquingKeysWith: { _, runtime in runtime })
        environment["PATH"] = runtimePath(environment["PATH"])
        return environment
    }

    static func runtimePath(_ existing: String?) -> String {
        let preferred = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"]
        let current = existing?.split(separator: ":").map(String.init) ?? []
        return Array(NSOrderedSet(array: preferred + current))
            .compactMap { $0 as? String }
            .joined(separator: ":")
    }
}
