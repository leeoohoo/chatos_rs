import Foundation

public struct ProductToolSkillDescriptor: Sendable, Equatable, Encodable {
    public let skillRef: String
    public let name: String
    public let role: String
    public let description: String

    enum CodingKeys: String, CodingKey {
        case skillRef = "skill_ref"
        case name, role, description
    }
}

public struct ProductToolSkillActivation: Sendable, Equatable {
    public let skillRef: String
    public let document: BundledAgentSkillDocument
}

/// One run-scoped activation state shared by every product-owned tool provider in that run.
/// Providers register stable binding IDs, while this session owns discovery refs, loading,
/// activation and resource access for the centrally maintained Skill bundle.
public actor ProductToolSkillSession {
    public enum SessionError: LocalizedError, Equatable {
        case unknownBinding(String)
        case unavailableSkill(String)
        case skillNotActivated(String)

        public var errorDescription: String? {
            switch self {
            case let .unknownBinding(id): "未知的产品工具 Skill binding：\(id)"
            case let .unavailableSkill(ref): "当前 Run 不允许使用这个产品 Skill：\(ref)"
            case let .skillNotActivated(ref): "请先激活产品 Skill：\(ref)"
            }
        }
    }

    public static let referencePrefix = "product-skill:"

    private let catalog: ToolSkillCoverageCatalog
    private var documentsByReference: [String: BundledAgentSkillDocument] = [:]
    private var activatedReferences: Set<String> = []

    public init(catalog: ToolSkillCoverageCatalog = .product) {
        self.catalog = catalog
    }

    public nonisolated static func recognizes(skillRef: String) -> Bool {
        skillRef.hasPrefix(referencePrefix)
    }

    public func register(providerID: String, skillBindingID: String) throws {
        let binding = try requiredBinding(
            providerID: providerID,
            skillBindingID: skillBindingID
        )
        for skillName in binding.requiredSkillNames {
            let reference = Self.reference(for: skillName)
            if documentsByReference[reference] == nil {
                documentsByReference[reference] = try BundledAgentSkillLoader.load(named: skillName)
            }
            if binding.activationPolicy == .runBound {
                activatedReferences.insert(reference)
            }
        }
    }

    public func descriptors(
        providerID: String,
        skillBindingID: String
    ) throws -> [ProductToolSkillDescriptor] {
        let binding = try requiredBinding(
            providerID: providerID,
            skillBindingID: skillBindingID
        )
        return try binding.requiredSkillNames.map { skillName in
            let reference = Self.reference(for: skillName)
            guard let document = documentsByReference[reference] else {
                throw SessionError.unavailableSkill(reference)
            }
            return .init(
                skillRef: reference,
                name: skillName,
                role: document.descriptor.role.rawValue,
                description: document.description
            )
        }
    }

    public func activate(skillRef: String) throws -> ProductToolSkillActivation {
        guard let document = documentsByReference[skillRef] else {
            throw SessionError.unavailableSkill(skillRef)
        }
        activatedReferences.insert(skillRef)
        return .init(skillRef: skillRef, document: document)
    }

    public func resourcePaths(skillRef: String) throws -> [String] {
        guard activatedReferences.contains(skillRef) else {
            throw SessionError.skillNotActivated(skillRef)
        }
        guard let document = documentsByReference[skillRef] else {
            throw SessionError.unavailableSkill(skillRef)
        }
        return document.resourcePaths
    }

    public func readResource(
        skillRef: String,
        relativePath: String,
        offset: Int = 0,
        maximumCharacters: Int = 12_000
    ) throws -> ProgressiveSkillFileLoader.TextPage {
        guard activatedReferences.contains(skillRef) else {
            throw SessionError.skillNotActivated(skillRef)
        }
        guard let document = documentsByReference[skillRef] else {
            throw SessionError.unavailableSkill(skillRef)
        }
        return try BundledAgentSkillLoader.readResource(
            skillName: document.descriptor.name,
            relativePath: relativePath,
            offset: offset,
            maximumCharacters: maximumCharacters
        )
    }

    public func missingSkills(
        providerID: String,
        skillBindingID: String
    ) throws -> [String] {
        let binding = try requiredBinding(
            providerID: providerID,
            skillBindingID: skillBindingID
        )
        guard binding.activationPolicy == .onDemand else { return [] }
        return binding.requiredSkillNames.filter {
            !activatedReferences.contains(Self.reference(for: $0))
        }
    }

    private func requiredBinding(
        providerID: String,
        skillBindingID: String
    ) throws -> ToolSkillBinding {
        guard let binding = catalog.binding(
            providerID: providerID,
            skillBindingID: skillBindingID
        ) else {
            throw SessionError.unknownBinding(providerID + ":" + skillBindingID)
        }
        return binding
    }

    private nonisolated static func reference(for skillName: String) -> String {
        referencePrefix + skillName
    }
}
