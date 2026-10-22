import Foundation

/// Compatibility bridge for signed npm MCP packages that bundled Skills but whose
/// generated ChatOS manifest predates the `skills` contribution. Discovery is narrow
/// and read-only: only direct `skills/<name>/SKILL.md` children are accepted.
enum NativePluginBundledSkillDiscovery {
    static let maximumSkillCount = 64

    static func relativeDirectories(
        installationRoot: URL,
        packageName: String,
        fileManager: FileManager = .default
    ) throws -> [String] {
        let roots = [
            (installationRoot.appendingPathComponent("skills", isDirectory: true), "skills"),
            (
                installationRoot
                    .appendingPathComponent("node_modules", isDirectory: true)
                    .appendingPathComponent(packageName, isDirectory: true)
                    .appendingPathComponent("skills", isDirectory: true),
                "node_modules/\(packageName)/skills"
            ),
        ]
        var result: [String] = []
        for (root, relativeRoot) in roots {
            var isDirectory: ObjCBool = false
            guard fileManager.fileExists(atPath: root.path, isDirectory: &isDirectory),
                  isDirectory.boolValue else { continue }
            let children = try fileManager.contentsOfDirectory(
                at: root,
                includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey],
                options: [.skipsHiddenFiles]
            )
            for child in children.sorted(by: { $0.lastPathComponent < $1.lastPathComponent }) {
                let values = try child.resourceValues(forKeys: [
                    .isDirectoryKey, .isSymbolicLinkKey,
                ])
                guard values.isDirectory == true,
                      values.isSymbolicLink != true,
                      fileManager.fileExists(
                        atPath: child.appendingPathComponent("SKILL.md").path
                      ) else { continue }
                result.append("\(relativeRoot)/\(child.lastPathComponent)")
                guard result.count <= maximumSkillCount else {
                    throw NativePluginRuntimeError.invalidManifest(
                        "Plugin 自动发现的 Skill 数量超过限制"
                    )
                }
            }
        }
        return Array(Set(result)).sorted()
    }
}
