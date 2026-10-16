import CryptoKit
import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    static func combinedSkillChecksum(
        main: NativeMainChatTaskSkillSnapshot,
        task: NativeMainChatTaskSkillSnapshot
    ) throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        // Catalog rendering is model-visible too: changes must not collide with an immutable revision.
        let items = try encoder.encode([
            main.prefixedInputItem, task.prefixedInputItem,
            .array(NativeLocalAgentPlatformToolCatalog.mainChatCompanionTools),
            NativeLocalAgentPlatformToolCatalog.attachmentReadTool,
            .array(NativeAgentCapabilityBrokerToolCatalog.localAgentCapabilityTools),
        ])
        return sha256(main.checksum + "\n" + task.checksum + "\n" + items.base64EncodedString())
    }

    static func skillPolicyRevision(baseRevision: String, skillBundleChecksum: String) -> String {
        "skill-policy-" + sha256(
            baseRevision + "\u{0}product-policy-v1\u{0}" + skillBundleChecksum
        ).dropFirst("sha256:".count)
    }

    static func capabilityRevision(
        baseRevision: String,
        skillBundleChecksum: String,
        mainTools: [LocalAgentJSONValue],
        taskTools: [LocalAgentJSONValue],
        taskPolicy: LocalAgentJSONValue
    ) throws -> String {
        // Freeze actual model-visible schemas and policy, not just option IDs:
        // a changed title/description/schema must never reuse an immutable revision.
        let identity = LocalAgentJSONValue.object([
            "revision_schema": .string("product-skills-v1"),
            "base_revision": .string(baseRevision),
            "skill_bundle_checksum": .string(skillBundleChecksum),
            "main_tools": .array(mainTools),
            "task_tools": .array(taskTools),
            "task_policy": taskPolicy,
        ])
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let digest = SHA256.hash(data: try encoder.encode(identity))
            .map { String(format: "%02x", $0) }.joined()
        return "local-agent-" + digest
    }

    static func mainChatTaskSkillSnapshot() throws -> NativeMainChatTaskSkillSnapshot {
        guard let taskBinding = ToolSkillCoverageCatalog.product.binding(
            providerID: ProductToolProviderID.mainChatTask,
            skillBindingID: ProductToolSkillBindingID.taskOrchestration
        ), taskBinding.activationPolicy == .onDemand,
           taskBinding.toolNames == Set(NativeLocalAgentPlatformToolCatalog.mainChatTaskToolNames),
           let controlBinding = ToolSkillCoverageCatalog.product.binding(
               providerID: ProductToolProviderID.localAgentChat,
               skillBindingID: ProductToolSkillBindingID.agentSkillControlPlane
           ), controlBinding.activationPolicy == .controlPlane,
           controlBinding.toolNames == Set([
               "agent_skill_activate", "agent_skill_read_resource",
           ])
        else {
            throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable
        }
        guard let surveyBinding = ToolSkillCoverageCatalog.product.binding(
            providerID: ProductToolProviderID.requirementSurvey,
            skillBindingID: ProductToolSkillBindingID.requirementDiscovery
        ) else { throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable }
        let snapshot = try productSkillBundle(
            bindings: [taskBinding, controlBinding, surveyBinding],
            policySkillName: "chatos-main-chat-invariants"
        )
        guard let taskDocument = snapshot.documents.first(where: {
            $0.name == taskBinding.specialistSkillName
        }) else {
            throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable
        }
        guard let surveyDocument = snapshot.documents.first(where: {
            $0.name == surveyBinding.specialistSkillName
        }) else { throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable }
        let marker = "[ChatOS Product Skill Catalog]"
        let text = """
        \(marker)
        The following Skills are available on demand. Requirement discovery is a companion capability, not a ninth Task tool.
        - `\(taskDocument.skillRef)` = **\(taskDocument.name)**: \(taskDocument.description)
        - `\(surveyDocument.skillRef)` = **\(surveyDocument.name)**: \(surveyDocument.description)

        Before calling a Task or Requirement Survey tool, call `agent_skill_activate` with its exact `skill_ref` and follow the returned instructions. Clarify material unknown goals or preferences before dispatching dependent work. The activation result already lists resources; read only relevant references with `agent_skill_read_resource`. If that result is no longer available after resume, activate the same exact ref again. Skill activation does not grant permissions or expand the user's requested scope.
        """
        return .init(
            checksum: snapshot.checksum,
            instructions: snapshot.policyInstructions,
            prefixedInputItem: skillCatalogInputItem(text),
            bundle: snapshot.bundle
        )
    }

    static func taskExecutionSkillSnapshot(
        externalMCPConfigs: [NativeLocalAgentExternalMCPConfig] = [],
        preservingExternalBindingsFrom persistedBundle: LocalAgentSkillBundleSnapshot? = nil
    ) throws -> NativeMainChatTaskSkillSnapshot {
        let catalog = ToolSkillCoverageCatalog.product
        let bindingKeys: [(String, String)] = [
            (ProductToolProviderID.capabilityBroker, ProductToolSkillBindingID.capabilityBroker),
            (ProductToolProviderID.projectRead, ProductToolSkillBindingID.projectRead),
            (ProductToolProviderID.projectWrite, ProductToolSkillBindingID.projectWrite),
            (ProductToolProviderID.terminal, ProductToolSkillBindingID.terminalCommandExecution),
            (ProductToolProviderID.terminal, ProductToolSkillBindingID.terminalProcessObservation),
            (ProductToolProviderID.terminal, ProductToolSkillBindingID.terminalProcessControl),
        ]
        let visibleToolNames = NativeLocalAgentPlatformToolCatalog.taskExecutionToolNames
        var bindings = try bindingKeys.map { providerID, bindingID in
            guard let binding = catalog.binding(
                providerID: providerID,
                skillBindingID: bindingID
            ) else {
                throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable
            }
            let toolNames = binding.toolNames.intersection(visibleToolNames)
            guard !toolNames.isEmpty else {
                throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable
            }
            return ToolSkillBinding(
                id: binding.id,
                providerID: binding.providerID,
                toolNames: toolNames,
                routerSkillName: binding.routerSkillName,
                specialistSkillName: binding.specialistSkillName,
                activationPolicy: binding.activationPolicy,
                surface: binding.surface
            )
        }
        bindings += try externalMCPConfigs.filter { !$0.tools.isEmpty }.map { config in
            ToolSkillBinding(
                id: try config.skillBindingID(), providerID: config.skillProviderID,
                toolNames: Set(config.tools.map(\.publicName)),
                routerSkillName: NativeLocalAgentExternalMCPConfig.skillName,
                specialistSkillName: NativeLocalAgentExternalMCPConfig.skillName,
                activationPolicy: .onDemand
            )
        }
        let preserved = persistedBundle?.toolSkillBindings.filter {
            $0.providerID.hasPrefix("chatos.external-mcp:")
        } ?? []
        guard externalMCPConfigs.isEmpty || preserved.isEmpty,
              preserved.allSatisfy({ binding in
                  let resourceID = String(binding.providerID.dropFirst("chatos.external-mcp:".count))
                  let prefix = "external-mcp:" + resourceID + ":"
                  let digest = String(binding.bindingID.dropFirst(prefix.count))
                  return !resourceID.isEmpty
                      && binding.bindingID.hasPrefix(prefix)
                      && digest.count == 64
                      && digest.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
                      && binding.activationPolicy == .onDemand
                      && binding.routerSkillRef == "product-skill:chatos-external-mcp"
                      && binding.specialistSkillRef == "product-skill:chatos-external-mcp"
                      && !binding.toolNames.isEmpty
                      && Set(binding.toolNames).count == binding.toolNames.count
                      && binding.toolNames.allSatisfy { $0.hasPrefix("external_mcp__") }
              }) else {
            throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable
        }
        bindings += preserved.map { binding in
            ToolSkillBinding(
                id: binding.bindingID, providerID: binding.providerID,
                toolNames: Set(binding.toolNames),
                routerSkillName: NativeLocalAgentExternalMCPConfig.skillName,
                specialistSkillName: NativeLocalAgentExternalMCPConfig.skillName,
                activationPolicy: .onDemand
            )
        }
        guard Set(bindings.map(\.id)).count == bindings.count,
              Set(bindings.flatMap { $0.toolNames }).count
                  == bindings.reduce(0, { $0 + $1.toolNames.count }) else {
            throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable
        }
        let snapshot = try productSkillBundle(
            bindings: bindings, policySkillName: "chatos-task-execution-invariants",
            schemaVersion: 2
        )
        guard let discoveryInstructions = snapshot.documents.first(where: {
            $0.name == "chatos-capability-discovery"
        })?.instructions else { throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable }
        let catalogLines = snapshot.documents
            .filter { $0.name != "chatos-capability-discovery" && $0.name != "chatos-task-execution-invariants" }
            .map { "- `\($0.skillRef)`: \($0.description)" }
            .joined(separator: "\n")
        let text = """
        [ChatOS Task Execution Product Skill Catalog]
        Run-bound capability discovery instructions:
        \(discoveryInstructions)

        Product Skills are progressively disclosed for project and terminal tools:
        \(catalogLines)

        Activate every exact `skill_ref` required by `capability_describe` or a `skill_activation_required` result before retrying the business tool. Skill activation does not add capabilities or bypass Task authorization, approval, project scope, or the fixed Plugin Release.
        For built-ins with `invocation_mode=direct_task_tool`, invoke the described native tool name directly after activation. Use `capability_invoke` for Plugin tools.
        External MCP tools are direct named tools, not Plugin broker options. Before calling one, activate `product-skill:chatos-external-mcp` if listed above. Its guidance covers safe remote invocation, not unavailable server-owned business Skills. A changed remote catalog/endpoint requires a new Task; never retry an uncertain remote mutation automatically.
        """
        return .init(
            checksum: snapshot.checksum,
            instructions: snapshot.policyInstructions,
            prefixedInputItem: skillCatalogInputItem(text),
            bundle: snapshot.bundle
        )
    }

    private static func productSkillBundle(
        bindings sourceBindings: [ToolSkillBinding],
        policySkillName: String,
        schemaVersion: UInt32 = 1
    ) throws -> (
        checksum: String,
        bundle: LocalAgentSkillBundleSnapshot,
        documents: [LocalAgentSkillDocumentSnapshot],
        policyInstructions: String
    ) {
        guard BundledAgentSkillCatalog.skill(named: policySkillName)?.role == .policy else {
            throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable
        }
        let skillNames = Set(sourceBindings.flatMap(\.requiredSkillNames)).union([policySkillName])
        let documents = try skillNames.sorted().map { skillName in
            let document = try BundledAgentSkillLoader.load(named: skillName)
            let resources = try document.resourcePaths.map { path in
                let page = try BundledAgentSkillLoader.readResource(
                    skillName: skillName,
                    relativePath: path,
                    maximumCharacters: 1_000_000
                )
                guard !page.truncated else {
                    throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable
                }
                return LocalAgentSkillResourceSnapshot(
                    relativePath: path,
                    content: page.content,
                    sha256: Self.sha256(page.content)
                )
            }
            return LocalAgentSkillDocumentSnapshot(
                skillRef: ProductToolSkillSession.referencePrefix + skillName,
                name: skillName,
                role: document.descriptor.role.rawValue,
                description: document.description,
                instructions: document.instructions,
                instructionsSHA256: Self.sha256(document.instructions),
                resources: resources
            )
        }
        func snapshotBinding(
            _ binding: ToolSkillBinding
        ) -> LocalAgentToolSkillBindingSnapshot {
            let activationPolicy: LocalAgentToolSkillActivationPolicy = switch binding.activationPolicy {
            case .onDemand: .onDemand
            case .runBound: .runBound
            case .controlPlane: .controlPlane
            }
            return .init(
                providerID: binding.providerID,
                bindingID: binding.id,
                toolNames: binding.toolNames.sorted(),
                routerSkillRef: ProductToolSkillSession.referencePrefix + binding.routerSkillName,
                specialistSkillRef: ProductToolSkillSession.referencePrefix
                    + binding.specialistSkillName,
                activationPolicy: activationPolicy
            )
        }
        let bindings = sourceBindings.map(snapshotBinding)
            .sorted { $0.bindingID < $1.bindingID }
        let unversionedBundle = LocalAgentSkillBundleSnapshot(
            schemaVersion: schemaVersion,
            bundleRevision: "unversioned",
            documents: documents,
            toolSkillBindings: bindings
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let digest = SHA256.hash(data: try encoder.encode(unversionedBundle))
            .map { String(format: "%02x", $0) }.joined()
        let revision = "sha256:\(digest)"
        let bundle = LocalAgentSkillBundleSnapshot(
            schemaVersion: schemaVersion,
            bundleRevision: revision,
            documents: documents,
            toolSkillBindings: bindings
        )
        guard let policy = documents.first(where: { $0.name == policySkillName }) else {
            throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable
        }
        return (revision, bundle, documents, policy.instructions)
    }

    private static func skillCatalogInputItem(_ text: String) -> LocalAgentJSONValue {
        .object([
            "type": .string("message"),
            "role": .string("system"),
            "content": .array([.object([
                "type": .string("input_text"),
                "text": .string(text),
            ])]),
        ])
    }

    static func replacingTaskSkillItem(
        in items: [LocalAgentJSONValue],
        with replacement: LocalAgentJSONValue
    ) -> [LocalAgentJSONValue] {
        [replacement] + items.filter { !isTaskSkillItem($0) }
    }

    static func replacingTaskExecutionSkillItem(
        in items: [LocalAgentJSONValue],
        with replacement: LocalAgentJSONValue
    ) -> [LocalAgentJSONValue] {
        [replacement] + items.filter { item in
            guard case .object(let message) = item,
                  case .array(let content)? = message["content"] else { return true }
            return !content.contains { part in
                guard case .object(let object) = part,
                      case .string(let text)? = object["text"] else { return false }
                return text.hasPrefix("[ChatOS Task Execution Product Skill Catalog]")
            }
        }
    }

    private static func isTaskSkillItem(_ item: LocalAgentJSONValue) -> Bool {
        guard case .object(let message) = item,
              case .array(let content)? = message["content"] else { return false }
        return content.contains { part in
            guard case .object(let object) = part,
                  case .string(let text)? = object["text"] else { return false }
            return text.hasPrefix("[ChatOS Product Skill Catalog]")
                || text.hasPrefix("[ChatOS Product Skill: chatos-async-task-orchestration]")
        }
    }

    static func sha256(_ text: String) -> String {
        let digest = SHA256.hash(data: Data(text.utf8))
            .map { String(format: "%02x", $0) }.joined()
        return "sha256:\(digest)"
    }

    static func encodedSkillBundle(
        _ bundle: LocalAgentSkillBundleSnapshot
    ) throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let data = try encoder.encode(bundle)
        guard let encoded = String(data: data, encoding: .utf8) else {
            throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable
        }
        return encoded
    }

}

