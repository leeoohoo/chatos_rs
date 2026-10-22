import Foundation
import Testing
@testable import ChatOSConnector

struct NativePluginBundledSkillDiscoveryTests {
    @Test
    func discoversSkillBundledInsideInstalledNpmDependency() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let skill = root
            .appendingPathComponent("node_modules/rea-agents/skills/reverse-engineer-anything")
        try FileManager.default.createDirectory(at: skill, withIntermediateDirectories: true)
        try Data("---\nname: reverse-engineer-anything\ndescription: test\n---\n".utf8)
            .write(to: skill.appendingPathComponent("SKILL.md"))
        defer { try? FileManager.default.removeItem(at: root) }

        #expect(try NativePluginBundledSkillDiscovery.relativeDirectories(
            installationRoot: root,
            packageName: "rea-agents"
        ) == ["node_modules/rea-agents/skills/reverse-engineer-anything"])
    }
}
