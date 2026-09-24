import Foundation

public struct BundledAgentSkillDocument: Sendable, Equatable {
    public let descriptor: BundledAgentSkillDescriptor
    public let description: String
    public let instructions: String
    public let resourcePaths: [String]
}

public enum BundledAgentSkillLoader {
    public enum LoaderError: LocalizedError, Equatable {
        case unknownSkill(String)
        case invalidEntrypoint(String)

        public var errorDescription: String? {
            switch self {
            case let .unknownSkill(name): "未知的产品内建 Skill：\(name)"
            case let .invalidEntrypoint(name): "产品内建 Skill 的 SKILL.md 无效：\(name)"
            }
        }
    }

    public static func load(named name: String) throws -> BundledAgentSkillDocument {
        guard let descriptor = BundledAgentSkillCatalog.skill(named: name) else {
            throw LoaderError.unknownSkill(name)
        }
        let root = try skillsRoot()
        let directory = root.appendingPathComponent(
            descriptor.relativeDirectory,
            isDirectory: true
        )
        try ProgressiveSkillFileLoader.validateDirectory(directory, beneath: root)
        let data = try ProgressiveSkillFileLoader.readRegularFile(
            directory.appendingPathComponent("SKILL.md"),
            beneath: root,
            maximumBytes: 256 * 1_024
        )
        guard let raw = String(data: data, encoding: .utf8) else {
            throw ProgressiveSkillFileLoader.LoaderError.invalidUTF8
        }
        let instructions = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !instructions.isEmpty,
              !descriptor.requiresFrontmatter || hasExpectedName(
                  instructions,
                  expectedName: descriptor.name
              ) else {
            throw LoaderError.invalidEntrypoint(name)
        }
        return .init(
            descriptor: descriptor,
            description: frontmatterValue("description", in: instructions) ?? "",
            instructions: instructions,
            resourcePaths: try resourcePaths(in: directory, beneath: root)
        )
    }

    public static func readResource(
        skillName: String,
        relativePath: String,
        offset: Int = 0,
        maximumCharacters: Int = 12_000
    ) throws -> ProgressiveSkillFileLoader.TextPage {
        let document = try load(named: skillName)
        let normalized = try ProgressiveSkillFileLoader.normalizedRelativePath(relativePath)
        guard document.resourcePaths.contains(normalized) else {
            throw LoaderError.invalidEntrypoint(skillName + ":" + normalized)
        }
        let root = try skillsRoot()
        let directory = root.appendingPathComponent(
            document.descriptor.relativeDirectory,
            isDirectory: true
        )
        return try ProgressiveSkillFileLoader.readTextPage(
            directory.appendingPathComponent(normalized),
            beneath: root,
            maximumBytes: 1_024 * 1_024,
            offset: offset,
            maximumCharacters: maximumCharacters
        )
    }

    public static func validateCatalog() throws {
        let names = BundledAgentSkillCatalog.skills.map(\.name)
        guard Set(names).count == names.count else {
            throw LoaderError.invalidEntrypoint("duplicate-catalog-name")
        }
        for name in names {
            _ = try load(named: name)
        }
        for binding in ToolSkillCoverageCatalog.product.bindings {
            for skillName in binding.requiredSkillNames {
                _ = try load(named: skillName)
            }
        }
    }

    private static func skillsRoot() throws -> URL {
        guard let root = Bundle.module.url(forResource: "Skills", withExtension: nil) else {
            throw LoaderError.invalidEntrypoint("Skills")
        }
        return root
    }

    private static func hasExpectedName(_ markdown: String, expectedName: String) -> Bool {
        frontmatterValue("name", in: markdown) == expectedName
    }

    private static func frontmatterValue(_ key: String, in markdown: String) -> String? {
        guard markdown.hasPrefix("---\n"),
              let end = markdown.dropFirst(4).range(of: "\n---") else { return nil }
        let prefix = key + ":"
        return markdown[..<end.lowerBound].split(separator: "\n").compactMap { line in
            let value = line.trimmingCharacters(in: .whitespaces)
            guard value.hasPrefix(prefix) else { return nil }
            return String(value.dropFirst(prefix.count))
                .trimmingCharacters(in: .whitespaces)
        }.first
    }

    private static func resourcePaths(in directory: URL, beneath root: URL) throws -> [String] {
        guard let enumerator = FileManager.default.enumerator(
            at: directory,
            includingPropertiesForKeys: [.isRegularFileKey, .isSymbolicLinkKey],
            options: [.skipsHiddenFiles]
        ) else { return [] }
        var paths: [String] = []
        for case let url as URL in enumerator where url.lastPathComponent != "SKILL.md" {
            let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
            guard values.isRegularFile == true, values.isSymbolicLink != true else { continue }
            _ = try ProgressiveSkillFileLoader.readRegularFile(
                url,
                beneath: root,
                maximumBytes: 1_024 * 1_024
            )
            paths.append(String(url.path.dropFirst(directory.path.count + 1)))
        }
        return paths.sorted()
    }
}
