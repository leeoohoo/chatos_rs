import CryptoKit
import ChatOSCore
import ChatOSModelCapabilities
import Foundation

extension NativeLocalConnectorService {
    static func defaultConversationModelConfigRef(
        in snapshots: [LocalAgentModelConfigSnapshot]
    ) -> String {
        snapshots.first(where: { !isLikelyGenerationOnlyModel($0.model) })?.modelConfigRef
            ?? snapshots[0].modelConfigRef
    }

    static func selectablePluginKeys(
        managedKeys: Set<String>,
        installedPlugins: [NativeInstalledAgentPlugin],
        records: [String: NativeInstalledPluginRecord]
    ) -> Set<String> {
        let installedPluginIDs = Set(installedPlugins.map(\.pluginID))
        let localKeys = records.values.compactMap { record -> String? in
            guard installedPluginIDs.contains(record.pluginID),
                  record.source != nil,
                  record.source != .marketplace else { return nil }
            return record.pluginKey?.trimmedNonEmpty
        }
        return managedKeys.union(localKeys)
    }

    private static func isLikelyGenerationOnlyModel(_ model: String) -> Bool {
        let value = model.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return [
            "gpt-image", "dall-e", "imagen", "imagegen", "image-generation",
            "sora", "veo", "kling", "seedance", "hailuo", "runway", "luma",
            "pixverse", "vidu", "cogvideo", "hunyuan-video",
        ].contains(where: value.contains)
    }

    static func externalMCPConfigs(
        _ mcps: [GatewayResolvedMCPDTO],
        selectedIDs: Set<String>
    ) -> [NativeLocalAgentExternalMCPConfig] {
        var usedToolNames = Set<String>()
        return mcps.sorted { $0.resource.id < $1.resource.id }.compactMap { item in
            guard selectedIDs.contains(item.resource.id),
                  let rawURL = item.resource.runtime.url?.trimmedNonEmpty,
                  let url = URL(string: rawURL) else { return nil }
            let serverName = item.resource.runtime.serverName?.trimmedNonEmpty
                ?? item.resource.name.trimmedNonEmpty
                ?? item.resource.id
            let tools = item.toolSnapshot.compactMap { raw -> NativeLocalAgentExternalMCPTool? in
                guard case .object(let object) = raw,
                      case .string(let upstreamName)? = object["name"],
                      let upstreamName = upstreamName.trimmedNonEmpty else { return nil }
                let description: String
                if case .string(let value)? = object["description"] { description = value }
                else { description = "" }
                let schema = object["inputSchema"] ?? object["input_schema"]
                    ?? .object(["type": .string("object")])
                return .init(
                    publicName: NativeLocalAgentExternalMCPNaming.disambiguatedToolName(
                        server: serverName,
                        tool: upstreamName,
                        resourceID: item.resource.id,
                        used: &usedToolNames
                    ),
                    upstreamName: upstreamName,
                    description: description,
                    inputSchema: schema,
                    remoteSkillGate: {
                        guard case let .object(metadata)? = object["_meta"] else { return nil }
                        return metadata["chatos/skillGate"]
                    }()
                )
            }
            return .init(
                resourceID: item.resource.id,
                serverName: serverName,
                url: url,
                headers: item.resource.runtime.headers,
                tools: tools
            )
        }
    }

    static func selectableBuiltinChoices(
        _ mcps: [GatewayResolvedMCPDTO]
    ) -> [NativeLocalAgentMCPChoice] {
        let candidates = mcps.filter {
            !$0.binding.required && $0.binding.enabled && $0.resource.enabled
              && $0.resource.runtime.resolvedBuiltinKind != nil
        }
        let available = Set(candidates.compactMap { $0.resource.runtime.resolvedBuiltinKind })
        return candidates.compactMap { item in
            guard let kind = item.resource.runtime.resolvedBuiltinKind else { return nil }
            if kind == "CodeMaintainerWrite", !available.contains("CodeMaintainerRead") {
                return nil
            }
            return .init(value: kind, title: Self.mcpChoiceTitle(item, value: kind))
        }
    }

    static func selectableExternalChoices(
        _ mcps: [GatewayResolvedMCPDTO]
    ) -> [NativeLocalAgentMCPChoice] {
        mcps.compactMap { item in
            guard !item.binding.required,
                  item.binding.enabled,
                  item.resource.enabled,
                  item.resource.runtime.resolvedBuiltinKind == nil,
                  item.resource.runtime.isExternalHTTP else { return nil }
            return .init(
                value: item.resource.id,
                title: Self.mcpChoiceTitle(item, value: item.resource.id)
            )
        }
    }

    private static func mcpChoiceTitle(
        _ item: GatewayResolvedMCPDTO,
        value: String
    ) -> String {
        let display = item.resource.displayName.trimmedNonEmpty
            ?? item.resource.name.trimmedNonEmpty
            ?? value
        var title = display == value ? value : "\(display) (\(value))"
        if let description = item.resource.description?.trimmedNonEmpty {
            title += " - \(description)"
        }
        let names = item.toolSnapshot.compactMap { tool -> String? in
            guard case .object(let object) = tool,
                  case .string(let name)? = object["name"] else { return nil }
            return name.trimmedNonEmpty
        }.prefix(12)
        if !names.isEmpty { title += " [tools: \(names.joined(separator: ", "))]" }
        return title
    }

    static func shouldUsePersistedCapability(after error: Error) -> Bool {
        guard let connectorError = error as? NativeConnectorError,
              case let .server(status, _) = connectorError else { return false }
        return status == 404
    }

    func restoredMainChatCapabilities(
        controlPlane: NativeLocalAgentControlPlaneClient,
        ownerUserID: String,
        modelSnapshots: [LocalAgentModelConfigSnapshot]
    ) async throws -> [String: LocalAgentCapabilityPolicySnapshot] {
        var result: [String: LocalAgentCapabilityPolicySnapshot] = [:]
        var validatedRevisions = Set<String>()
        for snapshot in modelSnapshots {
            guard let revision = state.localAgentCapabilityRevisionsByModelConfigID?[
                snapshot.modelConfigRef
            ]?.trimmedNonEmpty else { continue }
            do {
                let main = try await controlPlane.capabilities(
                    ownerUserID: ownerUserID,
                    profileKey: "main_chat",
                    capabilityPolicyRevision: revision
                )
                if !validatedRevisions.contains(revision) {
                    _ = try await controlPlane.capabilities(
                        ownerUserID: ownerUserID,
                        profileKey: "task_execution",
                        capabilityPolicyRevision: revision
                    )
                    validatedRevisions.insert(revision)
                }
                guard main.ownerUserID == ownerUserID,
                      main.profileKey == "main_chat",
                      main.capabilityPolicyRevision == revision else { continue }
                result[snapshot.modelConfigRef] = main
            } catch {
                continue
            }
        }
        if result.count < modelSnapshots.count {
            let latest = try await controlPlane.latestCapabilities(
                ownerUserID: ownerUserID,
                profileKey: "main_chat"
            )
            _ = try await controlPlane.capabilities(
                ownerUserID: ownerUserID,
                profileKey: "task_execution",
                capabilityPolicyRevision: latest.capabilityPolicyRevision
            )
            for snapshot in modelSnapshots where result[snapshot.modelConfigRef] == nil {
                result[snapshot.modelConfigRef] = latest
            }
        }
        guard result.count == modelSnapshots.count else {
            throw NativeLocalAgentBootstrapError.executionAgentUnavailable
        }
        return result
    }

    func modelRevision(_ model: GatewayModelConfigDTO) -> String {
        let fields = [
            model.id,
            model.provider,
            model.model,
            model.baseURL ?? "",
            model.taskThinkingLevel ?? "",
            model.temperature.map { String($0) } ?? "",
            model.maxOutputTokens.map { String($0) } ?? "",
            String(model.supportsImages ?? false),
            String(model.supportsResponses ?? false),
        ].joined(separator: "\u{0}")
        let digest = SHA256.hash(data: Data(fields.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        return "sha256-\(digest)"
    }

    static func modelThinkingLevel(_ level: String?, model: String) -> String? {
        if let capability = ModelReasoningCapabilities.forModel(model) {
            return capability.normalized(level) ?? capability.defaultLevel
        }
        return level?.trimmedNonEmpty
    }

    static func thinkingLevels(provider: String, model: String) -> [String] {
        if let capability = ModelReasoningCapabilities.forModel(model) { return capability.levels }
        return switch provider.trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased().replacingOccurrences(of: "-", with: "_") {
        case "gpt", "openai":
            ["none", "minimal", "low", "medium", "high", "xhigh"]
        case "deepseek":
            ["none", "low", "medium", "high", "max"]
        case "kimi", "kimik2", "moonshot":
            ["none", "auto", "low", "medium", "high", "xhigh"]
        default:
            ["none", "low", "medium", "high", "xhigh"]
        }
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
