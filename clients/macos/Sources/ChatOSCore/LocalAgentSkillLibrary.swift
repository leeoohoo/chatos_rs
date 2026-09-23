import Foundation

public enum LocalAgentSkillLibraryError: Error, LocalizedError, Equatable {
    case invalidField(String)
    case storage(String)

    public var errorDescription: String? {
        switch self {
        case let .invalidField(field):
            "无效的 Skill 字段：\(field)"
        case let .storage(message):
            "无法保存本地 Skill：\(message)"
        }
    }
}

/// Account-scoped local customizations layered over the immutable bundled ChatOS catalog.
/// Stable keys and categories remain program-owned; users can edit display metadata and the
/// complete runtime instructions without changing Agent or project identity bindings.
public final class LocalAgentSkillLibrary: @unchecked Sendable {
    private struct ProfessionOverride: Codable {
        let label: String
        let description: String
        let skillMarkdown: String
        let labelEN: String?
        let descriptionEN: String?
        let skillMarkdownEN: String?
    }

    private struct ProjectTypeOverride: Codable {
        let label: String
        let description: String
        let ruleMarkdown: String
        let labelEN: String?
        let descriptionEN: String?
        let ruleMarkdownEN: String?
    }

    private struct AccountOverrides: Codable {
        var professions: [String: ProfessionOverride] = [:]
        var projectTypes: [String: ProjectTypeOverride] = [:]
    }

    private struct Payload: Codable {
        var schemaVersion = 2
        var accounts: [String: AccountOverrides] = [:]
    }

    private let fileURL: URL
    private let lock = NSLock()
    private var payload: Payload

    public init(fileURL: URL) {
        self.fileURL = fileURL
        if let data = try? Data(contentsOf: fileURL),
           var decoded = try? JSONDecoder().decode(Payload.self, from: data),
           (1...2).contains(decoded.schemaVersion) {
            decoded.schemaVersion = 2
            payload = decoded
        } else {
            payload = .init()
        }
    }

    public func professions(ownerUserID: String) -> [LocalAgentProfessionDefinition] {
        withLock {
            let overrides = payload.accounts[ownerUserID]?.professions ?? [:]
            return LocalAgentSkillCatalog.professions.map { base in
                guard let value = overrides[base.key] else { return base }
                return .init(
                    key: base.key,
                    label: value.label,
                    labelEN: value.labelEN ?? base.labelEN,
                    description: value.description,
                    descriptionEN: value.descriptionEN ?? base.descriptionEN,
                    categoryKey: base.categoryKey,
                    categoryLabel: base.categoryLabel,
                    categoryLabelEN: base.categoryLabelEN,
                    skillName: base.skillName,
                    skillMarkdown: value.skillMarkdown,
                    skillMarkdownEN: value.skillMarkdownEN ?? base.skillMarkdownEN,
                    canCreateTasks: base.canCreateTasks
                )
            }
        }
    }

    public func projectTypes(ownerUserID: String) -> [LocalProjectTypeDefinition] {
        withLock {
            let overrides = payload.accounts[ownerUserID]?.projectTypes ?? [:]
            return LocalAgentSkillCatalog.projectTypes.map { base in
                guard let value = overrides[base.key] else { return base }
                return .init(
                    key: base.key,
                    label: value.label,
                    labelEN: value.labelEN ?? base.labelEN,
                    description: value.description,
                    descriptionEN: value.descriptionEN ?? base.descriptionEN,
                    categoryKey: base.categoryKey,
                    categoryLabel: base.categoryLabel,
                    categoryLabelEN: base.categoryLabelEN,
                    ruleMarkdown: value.ruleMarkdown,
                    ruleMarkdownEN: value.ruleMarkdownEN ?? base.ruleMarkdownEN
                )
            }
        }
    }

    public func profession(
        ownerUserID: String,
        key: String
    ) -> LocalAgentProfessionDefinition? {
        professions(ownerUserID: ownerUserID).first { $0.key == key }
    }

    public func projectType(
        ownerUserID: String,
        key: String
    ) -> LocalProjectTypeDefinition? {
        projectTypes(ownerUserID: ownerUserID).first { $0.key == key }
    }

    public func hasProfessionOverride(ownerUserID: String, key: String) -> Bool {
        withLock { payload.accounts[ownerUserID]?.professions[key] != nil }
    }

    public func hasProjectTypeOverride(ownerUserID: String, key: String) -> Bool {
        withLock { payload.accounts[ownerUserID]?.projectTypes[key] != nil }
    }

    public func updateProfession(
        ownerUserID: String,
        key: String,
        label: String,
        description: String,
        skillMarkdown: String
    ) throws {
        let current = profession(ownerUserID: ownerUserID, key: key)
            ?? LocalAgentSkillCatalog.profession(key: key)
        guard let current else {
            throw LocalAgentSkillLibraryError.invalidField("professionKey")
        }
        try updateProfessionBilingual(
            ownerUserID: ownerUserID,
            key: key,
            label: label,
            description: description,
            skillMarkdown: skillMarkdown,
            labelEN: current.labelEN,
            descriptionEN: current.descriptionEN,
            skillMarkdownEN: current.skillMarkdownEN
        )
    }

    public func updateProfessionBilingual(
        ownerUserID: String,
        key: String,
        label: String,
        description: String,
        skillMarkdown: String,
        labelEN: String,
        descriptionEN: String,
        skillMarkdownEN: String
    ) throws {
        guard LocalAgentSkillCatalog.profession(key: key) != nil else {
            throw LocalAgentSkillLibraryError.invalidField("professionKey")
        }
        let value = try ProfessionOverride(
            label: validate(label, field: "label", maximumLength: 120),
            description: validate(description, field: "description", maximumLength: 4_000),
            skillMarkdown: validate(
                skillMarkdown,
                field: "skillMarkdown",
                maximumLength: 500_000
            ),
            labelEN: validate(labelEN, field: "labelEN", maximumLength: 120),
            descriptionEN: validate(
                descriptionEN,
                field: "descriptionEN",
                maximumLength: 4_000
            ),
            skillMarkdownEN: validate(
                skillMarkdownEN,
                field: "skillMarkdownEN",
                maximumLength: 500_000
            )
        )
        try mutate { payload in
            var account = payload.accounts[ownerUserID] ?? .init()
            account.professions[key] = value
            payload.accounts[ownerUserID] = account
        }
    }

    public func updateProjectType(
        ownerUserID: String,
        key: String,
        label: String,
        description: String,
        ruleMarkdown: String
    ) throws {
        let current = projectType(ownerUserID: ownerUserID, key: key)
            ?? LocalAgentSkillCatalog.projectType(key: key)
        guard let current else {
            throw LocalAgentSkillLibraryError.invalidField("projectTypeKey")
        }
        try updateProjectTypeBilingual(
            ownerUserID: ownerUserID,
            key: key,
            label: label,
            description: description,
            ruleMarkdown: ruleMarkdown,
            labelEN: current.labelEN,
            descriptionEN: current.descriptionEN,
            ruleMarkdownEN: current.ruleMarkdownEN
        )
    }

    public func updateProjectTypeBilingual(
        ownerUserID: String,
        key: String,
        label: String,
        description: String,
        ruleMarkdown: String,
        labelEN: String,
        descriptionEN: String,
        ruleMarkdownEN: String
    ) throws {
        guard LocalAgentSkillCatalog.projectType(key: key) != nil else {
            throw LocalAgentSkillLibraryError.invalidField("projectTypeKey")
        }
        let value = try ProjectTypeOverride(
            label: validate(label, field: "label", maximumLength: 120),
            description: validate(description, field: "description", maximumLength: 4_000),
            ruleMarkdown: validate(
                ruleMarkdown,
                field: "ruleMarkdown",
                maximumLength: 500_000
            ),
            labelEN: validate(labelEN, field: "labelEN", maximumLength: 120),
            descriptionEN: validate(
                descriptionEN,
                field: "descriptionEN",
                maximumLength: 4_000
            ),
            ruleMarkdownEN: validate(
                ruleMarkdownEN,
                field: "ruleMarkdownEN",
                maximumLength: 500_000
            )
        )
        try mutate { payload in
            var account = payload.accounts[ownerUserID] ?? .init()
            account.projectTypes[key] = value
            payload.accounts[ownerUserID] = account
        }
    }

    public func resetProfession(ownerUserID: String, key: String) throws {
        try mutate { payload in
            guard var account = payload.accounts[ownerUserID] else { return }
            account.professions.removeValue(forKey: key)
            payload.accounts[ownerUserID] = account
        }
    }

    public func resetProjectType(ownerUserID: String, key: String) throws {
        try mutate { payload in
            guard var account = payload.accounts[ownerUserID] else { return }
            account.projectTypes.removeValue(forKey: key)
            payload.accounts[ownerUserID] = account
        }
    }

    private func validate(
        _ rawValue: String,
        field: String,
        maximumLength: Int
    ) throws -> String {
        let value = rawValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty,
              value.count <= maximumLength,
              !value.contains("\0") else {
            throw LocalAgentSkillLibraryError.invalidField(field)
        }
        return value
    }

    private func mutate(_ body: (inout Payload) -> Void) throws {
        lock.lock()
        defer { lock.unlock() }
        var next = payload
        body(&next)
        do {
            try FileManager.default.createDirectory(
                at: fileURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
            try encoder.encode(next).write(to: fileURL, options: .atomic)
            payload = next
        } catch {
            throw LocalAgentSkillLibraryError.storage(error.localizedDescription)
        }
    }

    private func withLock<Value>(_ body: () -> Value) -> Value {
        lock.lock()
        defer { lock.unlock() }
        return body()
    }
}
