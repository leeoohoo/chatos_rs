import CryptoKit
import Foundation

/// Shared filesystem contract for Plugin Skills and product-bundled Skills. It keeps both
/// surfaces on the same path, regular-file, symlink and pagination rules.
public enum ProgressiveSkillFileLoader {
    public struct TextPage: Sendable, Equatable {
        public let content: String
        public let offset: Int
        public let nextOffset: Int?
        public let truncated: Bool

        public init(
            content: String,
            offset: Int,
            nextOffset: Int?,
            truncated: Bool
        ) {
            self.content = content
            self.offset = offset
            self.nextOffset = nextOffset
            self.truncated = truncated
        }
    }

    public enum LoaderError: LocalizedError {
        case invalidPath
        case unavailableDirectory
        case unavailableFile
        case fileTooLarge
        case invalidUTF8
        case invalidOffset

        public var errorDescription: String? {
            switch self {
            case .invalidPath: "Skill 资源路径无效"
            case .unavailableDirectory: "Skill 目录不存在、不是目录或是符号链接"
            case .unavailableFile: "Skill 资源不存在、不是普通文件或是符号链接"
            case .fileTooLarge: "Skill 资源超过大小限制"
            case .invalidUTF8: "Skill 文本资源不是 UTF-8"
            case .invalidOffset: "Skill 资源 offset 无效"
            }
        }
    }

    public static func normalizedRelativePath(_ value: String) throws -> String {
        var path = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if path.hasPrefix("./") { path.removeFirst(2) }
        let parts = path.split(separator: "/", omittingEmptySubsequences: false)
        guard !path.isEmpty, !path.hasPrefix("/"), parts.allSatisfy({
            !$0.isEmpty && $0 != "." && $0 != ".."
        }) else { throw LoaderError.invalidPath }
        return path
    }

    public static func relativePath(
        of fileURL: URL,
        beneath directoryURL: URL
    ) throws -> String {
        let directory = directoryURL.resolvingSymlinksInPath().standardizedFileURL
        let file = fileURL.resolvingSymlinksInPath().standardizedFileURL
        let prefix = directory.path + "/"
        guard file.path.hasPrefix(prefix) else { throw LoaderError.invalidPath }
        return try normalizedRelativePath(String(file.path.dropFirst(prefix.count)))
    }

    public static func validateDirectory(
        _ directoryURL: URL,
        beneath rootURL: URL,
        fileManager: FileManager = .default
    ) throws {
        let root = rootURL.standardizedFileURL
        let directory = directoryURL.standardizedFileURL
        guard directory.path.hasPrefix(root.path + "/"),
              fileManager.fileExists(atPath: directory.path) else {
            throw LoaderError.unavailableDirectory
        }
        let values = try directory.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        guard values.isDirectory == true, values.isSymbolicLink != true else {
            throw LoaderError.unavailableDirectory
        }
    }

    public static func readRegularFile(
        _ fileURL: URL,
        beneath rootURL: URL,
        maximumBytes: Int,
        fileManager: FileManager = .default
    ) throws -> Data {
        let root = rootURL.standardizedFileURL
        let file = fileURL.standardizedFileURL
        guard file.path.hasPrefix(root.path + "/"), fileManager.fileExists(atPath: file.path) else {
            throw LoaderError.unavailableFile
        }
        let values = try file.resourceValues(forKeys: [
            .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey,
        ])
        guard values.isRegularFile == true, values.isSymbolicLink != true else {
            throw LoaderError.unavailableFile
        }
        guard values.fileSize ?? maximumBytes + 1 <= maximumBytes else {
            throw LoaderError.fileTooLarge
        }
        let data = try Data(contentsOf: file, options: .mappedIfSafe)
        guard data.count <= maximumBytes else { throw LoaderError.fileTooLarge }
        return data
    }

    public static func readTextPage(
        _ fileURL: URL,
        beneath rootURL: URL,
        maximumBytes: Int,
        offset: Int,
        maximumCharacters: Int,
        fileManager: FileManager = .default
    ) throws -> TextPage {
        let data = try readRegularFile(
            fileURL,
            beneath: rootURL,
            maximumBytes: maximumBytes,
            fileManager: fileManager
        )
        guard let text = String(data: data, encoding: .utf8) else { throw LoaderError.invalidUTF8 }
        return try textPage(
            text,
            offset: offset,
            maximumCharacters: maximumCharacters
        )
    }

    /// Shared character pagination for both bundled filesystem references and immutable
    /// in-memory run snapshots.
    public static func textPage(
        _ text: String,
        offset: Int,
        maximumCharacters: Int
    ) throws -> TextPage {
        let characters = Array(text)
        guard offset >= 0, offset <= characters.count else { throw LoaderError.invalidOffset }
        let limit = min(max(maximumCharacters, 1), 64_000)
        let end = min(offset + limit, characters.count)
        return .init(
            content: String(characters[offset..<end]),
            offset: offset,
            nextOffset: end < characters.count ? end : nil,
            truncated: end < characters.count
        )
    }
}

public enum LocalAgentProgressiveSkillCatalog {
    public struct Resource: Sendable, Equatable {
        public let relativePath: String
        public let kind: String
        public let sizeBytes: Int
        public let sha256: String
    }

    public struct Skill: Sendable, Equatable {
        public let skillRef: String
        public let name: String
        public let description: String
        public let role: String
        public let relatedSkills: [String]
        fileprivate let directory: String
    }

    public struct Activation: Sendable, Equatable {
        public let skill: Skill
        public let instructions: String
        public let instructionsSHA256: String
        public let resources: [Resource]
    }

    public enum CatalogError: LocalizedError {
        case unknownSkill
        case resourceNotFound

        public var errorDescription: String? {
            switch self {
            case .unknownSkill: "Skill 不在当前需求调研目录中"
            case .resourceNotFound: "Skill 资源不存在"
            }
        }
    }

    public static let requirementSurveyRouterRef = "SKreq-router"
    public static let requirementSurveyCreateRef = "SKreq-create"
    public static let requirementSurveyReadResultsRef = "SKreq-read"
    public static let requirementSurveyResolveRef = "SKreq-resolve"
    public static let requirementSurveyReviewExecutionRef = "SKreq-review"

    public static let requirementSurveySkills: [Skill] = [
        .init(
            skillRef: requirementSurveyRouterRef,
            name: "requirement-survey",
            description: "判断何时需要需求调研，并把创建、结果读取、方案生成或执行核对路由到对应专业 Skill。",
            role: "router",
            relatedSkills: [
                "requirement-survey-create", "requirement-survey-read-results",
                "requirement-survey-resolve", "requirement-survey-review-execution",
            ],
            directory: "requirement-survey"
        ),
        .init(
            skillRef: requirementSurveyCreateRef,
            name: "requirement-survey-create",
            description: "查重后创建一张以选择题为主、统一带备注入口的项目需求调研单。",
            role: "leaf",
            relatedSkills: [],
            directory: "requirement-survey-create"
        ),
        .init(
            skillRef: requirementSurveyReadResultsRef,
            name: "requirement-survey-read-results",
            description: "读取并区分 Human 选择、备注、既有方案与尚未确认事项。",
            role: "leaf",
            relatedSkills: [],
            directory: "requirement-survey-read-results"
        ),
        .init(
            skillRef: requirementSurveyResolveRef,
            name: "requirement-survey-resolve",
            description: "把 Human 已提交的调研结果转化为解决方案和结构化执行计划。",
            role: "leaf",
            relatedSkills: [],
            directory: "requirement-survey-resolve"
        ),
        .init(
            skillRef: requirementSurveyReviewExecutionRef,
            name: "requirement-survey-review-execution",
            description: "把正式执行计划与当前项目任务事实逐项核对。",
            role: "leaf",
            relatedSkills: [],
            directory: "requirement-survey-review-execution"
        ),
    ]

    public static func requirementSurveyCatalog(canWrite: Bool) -> [Skill] {
        requirementSurveySkills.filter { skill in
            canWrite || ![
                requirementSurveyCreateRef, requirementSurveyResolveRef,
            ].contains(skill.skillRef)
        }
    }

    public static func activateRequirementSurveySkill(skillRef: String) throws -> Activation {
        let skill = try requirementSurveySkill(skillRef: skillRef)
        let directory = try skillDirectory(skill)
        let data = try ProgressiveSkillFileLoader.readRegularFile(
            directory.appendingPathComponent("SKILL.md"),
            beneath: directory.deletingLastPathComponent(),
            maximumBytes: 256 * 1024
        )
        guard let instructionsText = String(data: data, encoding: .utf8) else {
            throw ProgressiveSkillFileLoader.LoaderError.invalidUTF8
        }
        let instructions = instructionsText
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return .init(
            skill: skill,
            instructions: instructions,
            instructionsSHA256: sha256(data),
            resources: try resources(for: skill)
        )
    }

    public static func requirementSurveyResources(skillRef: String) throws -> [Resource] {
        try resources(for: requirementSurveySkill(skillRef: skillRef))
    }

    public static func readRequirementSurveyResource(
        skillRef: String,
        relativePath: String,
        offset: Int,
        maximumCharacters: Int
    ) throws -> ProgressiveSkillFileLoader.TextPage {
        let skill = try requirementSurveySkill(skillRef: skillRef)
        let normalized = try ProgressiveSkillFileLoader.normalizedRelativePath(relativePath)
        guard try resources(for: skill).contains(where: { $0.relativePath == normalized }) else {
            throw CatalogError.resourceNotFound
        }
        let directory = try skillDirectory(skill)
        return try ProgressiveSkillFileLoader.readTextPage(
            directory.appendingPathComponent(normalized),
            beneath: directory.deletingLastPathComponent(),
            maximumBytes: 1024 * 1024,
            offset: offset,
            maximumCharacters: maximumCharacters
        )
    }

    private static func requirementSurveySkill(skillRef: String) throws -> Skill {
        guard let skill = requirementSurveySkills.first(where: { $0.skillRef == skillRef }) else {
            throw CatalogError.unknownSkill
        }
        return skill
    }

    private static func skillDirectory(_ skill: Skill) throws -> URL {
        guard let root = Bundle.module.url(forResource: "Skills", withExtension: nil) else {
            throw CatalogError.resourceNotFound
        }
        let directory = root.appendingPathComponent(skill.directory, isDirectory: true)
        try ProgressiveSkillFileLoader.validateDirectory(directory, beneath: root)
        return directory
    }

    private static func resources(for skill: Skill) throws -> [Resource] {
        let root = try skillDirectory(skill)
        guard let enumerator = FileManager.default.enumerator(
            at: root,
            includingPropertiesForKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey],
            options: [.skipsHiddenFiles]
        ) else { return [] }
        var output: [Resource] = []
        for case let url as URL in enumerator where url.lastPathComponent != "SKILL.md" {
            let values = try url.resourceValues(forKeys: [
                .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey,
            ])
            guard values.isRegularFile == true, values.isSymbolicLink != true else { continue }
            let relativePath = try ProgressiveSkillFileLoader.relativePath(
                of: url,
                beneath: root
            )
            let data = try ProgressiveSkillFileLoader.readRegularFile(
                url,
                beneath: root.deletingLastPathComponent(),
                maximumBytes: 1024 * 1024
            )
            output.append(.init(
                relativePath: relativePath,
                kind: relativePath.hasPrefix("references/") ? "reference" : "other",
                sizeBytes: data.count,
                sha256: sha256(data)
            ))
        }
        return output.sorted { $0.relativePath < $1.relativePath }
    }

    private static func sha256(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

}
