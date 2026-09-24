import Foundation

public struct PluginToolSkillGate: Decodable, Sendable, Equatable {
    public struct ArgumentSelector: Decodable, Sendable, Equatable {
        public let pointer: String
        public let map: [String: String]
    }

    public enum GateError: LocalizedError, Equatable {
        case invalidDeclaration
        case invalidSkillName(String)
        case invalidArguments
        case missingSelector(String)
        case invalidSelectorValue(String)
        case unmappedSelectorValue(String)

        public var code: String {
            switch self {
            case .invalidDeclaration: "invalid_declaration"
            case .invalidSkillName: "invalid_skill_name"
            case .invalidArguments: "invalid_arguments"
            case .missingSelector: "missing_selector"
            case .invalidSelectorValue: "invalid_selector_value"
            case .unmappedSelectorValue: "unmapped_selector_value"
            }
        }

        public var errorDescription: String? {
            switch self {
            case .invalidDeclaration: "Plugin Skill gate 声明无效"
            case let .invalidSkillName(name): "Plugin Skill gate 包含无效 Skill：\(name)"
            case .invalidArguments: "Plugin Skill gate 要求工具参数为 JSON 对象"
            case let .missingSelector(pointer): "Plugin Skill gate 缺少选择参数：\(pointer)"
            case let .invalidSelectorValue(pointer):
                "Plugin Skill gate 选择参数必须是字符串：\(pointer)"
            case let .unmappedSelectorValue(value):
                "Plugin Skill gate 没有匹配选择值：\(value)"
            }
        }
    }

    public let allOf: [String]
    public let selectByArgument: ArgumentSelector?

    private enum CodingKeys: String, CodingKey { case allOf, selectByArgument }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        allOf = try values.decodeIfPresent([String].self, forKey: .allOf) ?? []
        selectByArgument = try values.decodeIfPresent(
            ArgumentSelector.self,
            forKey: .selectByArgument
        )
    }

    public static func decode(_ data: Data) throws -> Self {
        do {
            guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                  Set(object.keys).isSubset(of: ["allOf", "selectByArgument"]) else {
                throw GateError.invalidDeclaration
            }
            if let selector = object["selectByArgument"] {
                guard let selector = selector as? [String: Any],
                      Set(selector.keys).isSubset(of: ["pointer", "map"]) else {
                    throw GateError.invalidDeclaration
                }
            }
            let gate = try JSONDecoder().decode(Self.self, from: data)
            try gate.validate()
            return gate
        } catch let error as GateError {
            throw error
        } catch {
            throw GateError.invalidDeclaration
        }
    }

    public var catalogSkillNames: [String] {
        Array(Set(allOf + (selectByArgument.map { Array($0.map.values) } ?? []))).sorted()
    }

    public func requiredSkillNames(arguments: Data) throws -> [String] {
        try validate()
        guard let root = try JSONSerialization.jsonObject(with: arguments) as? [String: Any] else {
            throw GateError.invalidArguments
        }
        var required = Set(allOf)
        if let selector = selectByArgument {
            guard let selected = Self.value(at: selector.pointer, in: root) else {
                throw GateError.missingSelector(selector.pointer)
            }
            guard let selected = selected as? String else {
                throw GateError.invalidSelectorValue(selector.pointer)
            }
            guard let skillName = selector.map[selected] else {
                throw GateError.unmappedSelectorValue(selected)
            }
            required.insert(skillName)
        }
        return required.sorted()
    }

    private func validate() throws {
        guard !allOf.isEmpty || selectByArgument != nil else {
            throw GateError.invalidDeclaration
        }
        for name in catalogSkillNames where !Self.isValidSkillName(name) {
            throw GateError.invalidSkillName(name)
        }
        if let selector = selectByArgument {
            guard Self.isValidJSONPointer(selector.pointer), !selector.map.isEmpty,
                  selector.map.keys.allSatisfy({ !$0.trimmingCharacters(in: .whitespaces).isEmpty }) else {
                throw GateError.invalidDeclaration
            }
        }
    }

    fileprivate static func isValidSkillName(_ value: String) -> Bool {
        !value.isEmpty && value.utf8.count <= 64
            && value.utf8.allSatisfy {
                ($0 >= 97 && $0 <= 122) || ($0 >= 48 && $0 <= 57) || $0 == 45
            }
            && !value.hasPrefix("-") && !value.hasSuffix("-") && !value.contains("--")
    }

    private static func isValidJSONPointer(_ value: String) -> Bool {
        guard value.hasPrefix("/") else { return false }
        let bytes = Array(value.utf8)
        var index = 0
        while index < bytes.count {
            if bytes[index] == 126 {
                index += 1
                guard index < bytes.count, bytes[index] == 48 || bytes[index] == 49 else {
                    return false
                }
            }
            index += 1
        }
        return true
    }

    private static func value(at pointer: String, in root: Any) -> Any? {
        var current: Any = root
        for rawPart in pointer.dropFirst().split(separator: "/", omittingEmptySubsequences: false) {
            let part = rawPart.replacingOccurrences(of: "~1", with: "/")
                .replacingOccurrences(of: "~0", with: "~")
            if let object = current as? [String: Any], let next = object[part] {
                current = next
            } else if let array = current as? [Any], let index = Int(part), array.indices.contains(index) {
                current = array[index]
            } else {
                return nil
            }
        }
        return current
    }
}

public struct PluginSkillSnapshot: Sendable {
    public struct Document: Sendable, Equatable {
        public let name: String
        public let description: String
        public let role: String
        public let instructions: String
        public let resourcePaths: [String]
        fileprivate let resources: [String: String]
    }

    public enum SnapshotError: LocalizedError, Equatable {
        case invalidCatalog
        case invalidSkill(String)
        case unknownSkill(String)
        case unavailableResource(String)

        public var errorDescription: String? {
            switch self {
            case .invalidCatalog: "Plugin Skill 目录无效"
            case let .invalidSkill(name): "Plugin Skill 文档无效：\(name)"
            case let .unknownSkill(name): "Plugin Skill 不在当前固定快照中：\(name)"
            case let .unavailableResource(path): "Plugin Skill 资源不可用：\(path)"
            }
        }
    }

    private static let maximumSkillCount = 128
    private static let maximumInstructionsBytes = 256 * 1_024
    private static let maximumResourceBytes = 1_024 * 1_024
    private static let maximumResourcesPerSkill = 256
    private static let maximumCatalogBytes = 16 * 1_024 * 1_024

    private let documentsByName: [String: Document]

    public var skillNames: Set<String> { Set(documentsByName.keys) }

    public static func load(
        installationRoot: URL,
        relativeSkillDirectories: [String],
        fileManager: FileManager = .default
    ) throws -> Self {
        guard relativeSkillDirectories.count <= maximumSkillCount else {
            throw SnapshotError.invalidCatalog
        }
        let root = installationRoot.standardizedFileURL
        var documents: [String: Document] = [:]
        var totalBytes = 0
        for rawPath in relativeSkillDirectories {
            let relativePath = try ProgressiveSkillFileLoader.normalizedRelativePath(rawPath)
            let directory = root.appendingPathComponent(relativePath, isDirectory: true)
            try ProgressiveSkillFileLoader.validateDirectory(
                directory,
                beneath: root,
                fileManager: fileManager
            )
            let data = try ProgressiveSkillFileLoader.readRegularFile(
                directory.appendingPathComponent("SKILL.md"),
                beneath: root,
                maximumBytes: maximumInstructionsBytes,
                fileManager: fileManager
            )
            totalBytes += data.count
            guard totalBytes <= maximumCatalogBytes,
                  let raw = String(data: data, encoding: .utf8) else {
                throw SnapshotError.invalidCatalog
            }
            let parsed = try parseDocument(raw, expectedName: directory.lastPathComponent)
            let resources = try textResources(
                in: directory,
                beneath: root,
                totalBytes: &totalBytes,
                fileManager: fileManager
            )
            let document = Document(
                name: parsed.name,
                description: parsed.description,
                role: parsed.role,
                instructions: parsed.instructions,
                resourcePaths: resources.keys.sorted(),
                resources: resources
            )
            guard documents.updateValue(document, forKey: document.name) == nil else {
                throw SnapshotError.invalidSkill(document.name)
            }
        }
        return .init(documentsByName: documents)
    }

    public func document(named name: String) throws -> Document {
        guard let document = documentsByName[name] else {
            throw SnapshotError.unknownSkill(name)
        }
        return document
    }

    public func readResource(
        skillName: String,
        relativePath: String,
        offset: Int = 0,
        maximumCharacters: Int = 12_000
    ) throws -> ProgressiveSkillFileLoader.TextPage {
        let document = try document(named: skillName)
        let path = try ProgressiveSkillFileLoader.normalizedRelativePath(relativePath)
        guard let content = document.resources[path] else {
            throw SnapshotError.unavailableResource(path)
        }
        return try ProgressiveSkillFileLoader.textPage(
            content,
            offset: offset,
            maximumCharacters: maximumCharacters
        )
    }

    private static func parseDocument(
        _ raw: String,
        expectedName: String
    ) throws -> (name: String, description: String, role: String, instructions: String) {
        let normalized = raw.replacingOccurrences(of: "\r\n", with: "\n")
        guard normalized.hasPrefix("---\n"),
              let end = normalized.dropFirst(4).range(of: "\n---\n") else {
            throw SnapshotError.invalidSkill(expectedName)
        }
        let frontmatter = String(normalized[normalized.index(normalized.startIndex, offsetBy: 4)..<end.lowerBound])
        let name = scalar("name", in: frontmatter) ?? ""
        let description = scalar("description", in: frontmatter) ?? ""
        let role = scalar("chatos.role", in: frontmatter) ?? "leaf"
        let body = normalized[end.upperBound...].trimmingCharacters(in: .whitespacesAndNewlines)
        guard name == expectedName, !description.isEmpty, !body.isEmpty,
              role == "router" || role == "leaf",
              PluginToolSkillGate.isValidSkillName(name) else {
            throw SnapshotError.invalidSkill(expectedName)
        }
        return (
            name,
            description,
            role,
            normalized.trimmingCharacters(in: .whitespacesAndNewlines)
        )
    }

    private static func scalar(_ key: String, in frontmatter: String) -> String? {
        let prefix = key + ":"
        guard let rawValue = frontmatter.split(separator: "\n").compactMap({ line -> String? in
            let line = line.trimmingCharacters(in: .whitespaces)
            guard line.hasPrefix(prefix) else { return nil }
            return String(line.dropFirst(prefix.count)).trimmingCharacters(in: .whitespaces)
        }).first else { return nil }
        if rawValue.count >= 2,
           (rawValue.hasPrefix("\"") && rawValue.hasSuffix("\"")
            || rawValue.hasPrefix("'") && rawValue.hasSuffix("'")) {
            return String(rawValue.dropFirst().dropLast())
        }
        return rawValue
    }

    private static func textResources(
        in directory: URL,
        beneath root: URL,
        totalBytes: inout Int,
        fileManager: FileManager
    ) throws -> [String: String] {
        guard let enumerator = fileManager.enumerator(
            at: directory,
            includingPropertiesForKeys: [
                .isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey,
            ],
            options: [.skipsHiddenFiles]
        ) else { return [:] }
        var resources: [String: String] = [:]
        var seenResources = 0
        while let url = enumerator.nextObject() as? URL {
            let values = try url.resourceValues(forKeys: [
                .isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey,
            ])
            if values.isSymbolicLink == true {
                enumerator.skipDescendants()
                continue
            }
            guard values.isRegularFile == true, url.lastPathComponent != "SKILL.md" else {
                continue
            }
            seenResources += 1
            guard seenResources <= maximumResourcesPerSkill else {
                throw SnapshotError.invalidCatalog
            }
            let path = String(url.standardizedFileURL.path.dropFirst(directory.path.count + 1))
            let topLevel = path.split(separator: "/").first.map(String.init) ?? ""
            guard topLevel != "assets", topLevel != "scripts" else { continue }
            let data = try ProgressiveSkillFileLoader.readRegularFile(
                url,
                beneath: root,
                maximumBytes: maximumResourceBytes,
                fileManager: fileManager
            )
            totalBytes += data.count
            guard totalBytes <= maximumCatalogBytes else { throw SnapshotError.invalidCatalog }
            guard let text = String(data: data, encoding: .utf8) else { continue }
            resources[path] = text
        }
        return resources
    }
}

public actor PluginToolSkillSession {
    public enum SessionError: LocalizedError, Equatable {
        case skillNotActivated(String)

        public var errorDescription: String? {
            switch self {
            case let .skillNotActivated(name): "Plugin Skill 尚未激活：\(name)"
            }
        }
    }

    public let snapshot: PluginSkillSnapshot
    private var activatedNames: Set<String> = []

    public init(snapshot: PluginSkillSnapshot) { self.snapshot = snapshot }

    public func activate(named name: String) throws -> PluginSkillSnapshot.Document {
        let document = try snapshot.document(named: name)
        activatedNames.insert(name)
        return document
    }

    public func readResource(
        skillName: String,
        relativePath: String,
        offset: Int = 0,
        maximumCharacters: Int = 12_000
    ) throws -> ProgressiveSkillFileLoader.TextPage {
        guard activatedNames.contains(skillName) else {
            throw SessionError.skillNotActivated(skillName)
        }
        return try snapshot.readResource(
            skillName: skillName,
            relativePath: relativePath,
            offset: offset,
            maximumCharacters: maximumCharacters
        )
    }

    public func missingSkills(
        for gate: PluginToolSkillGate,
        arguments: Data
    ) throws -> [String] {
        let required = Set(try gate.requiredSkillNames(arguments: arguments))
        return required.subtracting(activatedNames).sorted()
    }
}
