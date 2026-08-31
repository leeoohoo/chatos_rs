import Foundation

enum NativeBundledToolLocator {
    static func executable(named name: String) -> URL? {
        let architecture = currentArchitecture
        let candidates = [
            Bundle.main.resourceURL?
                .appendingPathComponent("Tools", isDirectory: true)
                .appendingPathComponent(architecture, isDirectory: true)
                .appendingPathComponent(name),
            repositorySupportDirectory()?
                .appendingPathComponent("Tools", isDirectory: true)
                .appendingPathComponent(architecture, isDirectory: true)
                .appendingPathComponent(name),
        ].compactMap { $0 }

        return candidates.first { FileManager.default.isExecutableFile(atPath: $0.path) }
    }

    private static var currentArchitecture: String {
        #if arch(arm64)
        "darwin-arm64"
        #elseif arch(x86_64)
        "darwin-x86_64"
        #else
        "darwin-unknown"
        #endif
    }

    private static func repositorySupportDirectory() -> URL? {
        var directory = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        for _ in 0..<5 {
            let candidate = directory.appendingPathComponent("Support", isDirectory: true)
            if FileManager.default.fileExists(atPath: candidate.path) {
                return candidate
            }
            directory.deleteLastPathComponent()
        }
        return nil
    }
}
