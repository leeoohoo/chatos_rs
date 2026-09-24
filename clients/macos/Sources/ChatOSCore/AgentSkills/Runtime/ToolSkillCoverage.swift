import Foundation

/// Skill content ownership. Plugin Skill files remain in the signed plugin release, while their
/// bindings are interpreted by the same coverage and activation runtime as product Skills.
public enum ToolSkillSurface: String, Codable, Sendable {
    case productBuiltIn = "product_builtin"
    case plugin
}

public enum ToolSkillActivationPolicy: String, Codable, Sendable {
    /// Show only routing metadata initially; load the Skill body when the model activates it.
    case onDemand = "on_demand"
    /// Bind the Skill to the run before any covered tool is exposed.
    case runBound = "run_bound"
}

/// A stable mapping from a provider-owned binding ID to centrally owned Skill content.
public struct ToolSkillBinding: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let providerID: String
    public let toolNames: Set<String>
    public let routerSkillName: String
    public let specialistSkillName: String
    public let activationPolicy: ToolSkillActivationPolicy
    public let surface: ToolSkillSurface

    public init(
        id: String,
        providerID: String,
        toolNames: Set<String>,
        routerSkillName: String,
        specialistSkillName: String,
        activationPolicy: ToolSkillActivationPolicy = .onDemand,
        surface: ToolSkillSurface = .productBuiltIn
    ) {
        self.id = id
        self.providerID = providerID
        self.toolNames = toolNames
        self.routerSkillName = routerSkillName
        self.specialistSkillName = specialistSkillName
        self.activationPolicy = activationPolicy
        self.surface = surface
    }

    public var requiredSkillNames: [String] {
        routerSkillName == specialistSkillName
            ? [routerSkillName]
            : [routerSkillName, specialistSkillName]
    }
}

/// Dependency-neutral view of a model-visible tool. Connector targets adapt their concrete tool
/// definitions into this value so ChatOSCore does not need to depend on ChatOSAgentRuntime.
public struct ToolSkillCoverageInput: Sendable, Equatable {
    public let providerID: String?
    public let toolName: String
    public let skillBindingID: String?

    public init(providerID: String?, toolName: String, skillBindingID: String?) {
        self.providerID = providerID
        self.toolName = toolName
        self.skillBindingID = skillBindingID
    }
}

public struct ToolSkillCoverageIssue: Sendable, Equatable {
    public enum Kind: String, Sendable {
        case missingProviderID = "missing_provider_id"
        case missingBindingID = "missing_binding_id"
        case unknownBinding = "unknown_binding"
        case providerMismatch = "provider_mismatch"
        case toolNotDeclared = "tool_not_declared"
    }

    public let kind: Kind
    public let toolName: String
    public let providerID: String?
    public let skillBindingID: String?
}

public struct ToolSkillCoverageReport: Sendable, Equatable {
    public let totalTools: Int
    public let coveredTools: Int
    public let issues: [ToolSkillCoverageIssue]

    public var isComplete: Bool { issues.isEmpty && coveredTools == totalTools }
}

public struct ToolSkillCoverageCatalog: Sendable {
    public enum CatalogError: Error, Equatable {
        case emptyField(String)
        case emptyToolSet(String)
        case duplicateBinding(String)
        case duplicateProviderTool(String)
    }

    public let bindings: [ToolSkillBinding]
    private let bindingsByID: [String: ToolSkillBinding]

    public init(bindings: [ToolSkillBinding]) throws {
        var byID: [String: ToolSkillBinding] = [:]
        var providerTools: Set<String> = []
        for binding in bindings {
            for (field, value) in [
                ("id", binding.id),
                ("providerID", binding.providerID),
                ("routerSkillName", binding.routerSkillName),
                ("specialistSkillName", binding.specialistSkillName),
            ] where value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                throw CatalogError.emptyField(field)
            }
            guard !binding.toolNames.isEmpty else {
                throw CatalogError.emptyToolSet(binding.id)
            }
            guard byID.updateValue(binding, forKey: binding.id) == nil else {
                throw CatalogError.duplicateBinding(binding.id)
            }
            for toolName in binding.toolNames {
                let key = binding.providerID + "\u{0}" + toolName
                guard providerTools.insert(key).inserted else {
                    throw CatalogError.duplicateProviderTool(key)
                }
            }
        }
        self.bindings = bindings
        bindingsByID = byID
    }

    public func binding(
        providerID: String,
        skillBindingID: String
    ) -> ToolSkillBinding? {
        guard let binding = bindingsByID[skillBindingID],
              binding.providerID == providerID else { return nil }
        return binding
    }

    /// Audit mode is intentionally non-enforcing during migration. The same report becomes the
    /// release gate after every model-visible provider has adopted stable binding IDs.
    public func audit(_ tools: [ToolSkillCoverageInput]) -> ToolSkillCoverageReport {
        var covered = 0
        var issues: [ToolSkillCoverageIssue] = []
        for tool in tools {
            let issue: ToolSkillCoverageIssue.Kind?
            if tool.providerID?.isEmpty != false {
                issue = .missingProviderID
            } else if tool.skillBindingID?.isEmpty != false {
                issue = .missingBindingID
            } else if let bindingID = tool.skillBindingID,
                      let binding = bindingsByID[bindingID] {
                if binding.providerID != tool.providerID {
                    issue = .providerMismatch
                } else if !binding.toolNames.contains(tool.toolName) {
                    issue = .toolNotDeclared
                } else {
                    issue = nil
                    covered += 1
                }
            } else {
                issue = .unknownBinding
            }
            if let issue {
                issues.append(.init(
                    kind: issue,
                    toolName: tool.toolName,
                    providerID: tool.providerID,
                    skillBindingID: tool.skillBindingID
                ))
            }
        }
        return .init(totalTools: tools.count, coveredTools: covered, issues: issues)
    }
}
