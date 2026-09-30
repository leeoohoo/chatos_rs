import ChatOSCore
import Foundation

public struct NativeLocalAgentConversationRuntimeSelection: Sendable, Equatable {
    public let settings: LocalAgentConversationRuntimeSettings
    public let modelSnapshot: LocalAgentModelConfigSnapshot
}

public actor NativeLocalAgentConversationRuntimeSettingsService:
    ConversationRuntimeSettingsServicing
{
    private struct Context: Sendable {
        let ownerUserID: String
        let modelSnapshots: [LocalAgentModelConfigSnapshot]
        let modelOptions: [ConversationModelOption]
    }

    private let client: NativeLocalAgentConversationRuntimeSettingsClient
    private var context: Context?

    public init(host: any LocalAgentHostClientServicing) {
        client = NativeLocalAgentConversationRuntimeSettingsClient(host: host)
    }

    public func configure(
        ownerUserID: String,
        bootstrap: NativeLocalAgentBootstrapResult
    ) throws {
        guard !bootstrap.modelSnapshots.isEmpty,
              bootstrap.modelOptions.count == bootstrap.modelSnapshots.count,
              bootstrap.modelSnapshots.allSatisfy({ $0.ownerUserID == ownerUserID }),
              bootstrap.modelOptions.allSatisfy({ option in
                  bootstrap.modelSnapshots.contains(where: { $0.modelConfigRef == option.id })
              }) else {
            throw NativeLocalAgentConversationRuntimeSettingsError.notConfigured
        }
        context = .init(
            ownerUserID: ownerUserID,
            modelSnapshots: bootstrap.modelSnapshots,
            modelOptions: bootstrap.modelOptions
        )
    }

    public func reset() {
        context = nil
    }

    public func fetchSettings(sessionID: String) async throws -> ConversationRuntimeSettings {
        try domainSettings(from: await ensureSettings(sessionID: sessionID))
    }

    public func fetchAvailableModels() async throws -> [ConversationModelOption] {
        try requireContext().modelOptions
    }

    public func updateModel(
        sessionID: String,
        modelID: String
    ) async throws -> ConversationRuntimeSettings {
        let context = try requireContext()
        guard let snapshot = context.modelSnapshots.first(where: {
            $0.modelConfigRef == modelID
        }) else {
            throw NativeLocalAgentConversationRuntimeSettingsError.unknownModel
        }
        let current = try await ensureSettings(sessionID: sessionID)
        let level = defaultLevel(for: snapshot)
        return try domainSettings(from: await put(
            current: current,
            snapshot: snapshot,
            selectedThinkingLevel: level,
            remoteConnectionID: current.remoteConnectionID,
            reasoningEnabled: level != "none"
        ))
    }

    public func updateRemoteConnection(
        sessionID: String,
        connectionID: String?
    ) async throws -> ConversationRuntimeSettings {
        let current = try await ensureSettings(sessionID: sessionID)
        let snapshot = try snapshot(for: current)
        return try domainSettings(from: await put(
            current: current,
            snapshot: snapshot,
            selectedThinkingLevel: current.selectedThinkingLevel,
            remoteConnectionID: connectionID,
            reasoningEnabled: current.reasoningEnabled
        ))
    }

    public func updateReasoning(
        sessionID: String,
        enabled: Bool
    ) async throws -> ConversationRuntimeSettings {
        let current = try await ensureSettings(sessionID: sessionID)
        let snapshot = try snapshot(for: current)
        let option = try option(for: snapshot.modelConfigRef)
        let level = enabled
            ? enabledLevel(current: current, snapshot: snapshot, option: option)
            : "none"
        return try domainSettings(from: await put(
            current: current,
            snapshot: snapshot,
            selectedThinkingLevel: level,
            remoteConnectionID: current.remoteConnectionID,
            reasoningEnabled: enabled && level != "none"
        ))
    }

    public func updateReasoningLevel(
        sessionID: String,
        level: String,
        enabled: Bool
    ) async throws -> ConversationRuntimeSettings {
        let current = try await ensureSettings(sessionID: sessionID)
        let snapshot = try snapshot(for: current)
        let option = try option(for: snapshot.modelConfigRef)
        let normalized = normalizedThinkingLevel(level, provider: snapshot.provider)
        guard option.thinkingLevels.contains(normalized),
              enabled == (normalized != "none") else {
            throw NativeLocalAgentConversationRuntimeSettingsError.unsupportedThinkingLevel
        }
        return try domainSettings(from: await put(
            current: current,
            snapshot: snapshot,
            selectedThinkingLevel: normalized,
            remoteConnectionID: current.remoteConnectionID,
            reasoningEnabled: enabled
        ))
    }

    public func resolveSelection(
        sessionID: String
    ) async throws -> NativeLocalAgentConversationRuntimeSelection {
        let settings = try await ensureSettings(sessionID: sessionID)
        return .init(settings: settings, modelSnapshot: try snapshot(for: settings))
    }

    private func ensureSettings(
        sessionID: String
    ) async throws -> LocalAgentConversationRuntimeSettings {
        let context = try requireContext()
        do {
            return try await client.get(
                ownerUserID: context.ownerUserID,
                conversationID: sessionID
            )
        } catch let error as NativeLocalAgentHostError {
            guard case let .hostError(code, _, _) = error, code == "not_found" else {
                throw error
            }
        }
        guard let snapshot = context.modelSnapshots.first else {
            throw NativeLocalAgentConversationRuntimeSettingsError.notConfigured
        }
        let level = defaultLevel(for: snapshot)
        do {
            return try await client.put(.init(
                ownerUserID: context.ownerUserID,
                conversationID: sessionID,
                selectedModelConfigRef: snapshot.modelConfigRef,
                selectedModelConfigRevision: snapshot.modelConfigRevision,
                selectedThinkingLevel: level,
                remoteConnectionID: nil,
                reasoningEnabled: level != "none",
                expectedVersion: nil
            ))
        } catch let error as NativeLocalAgentHostError {
            guard case let .hostError(code, _, _) = error, code == "conflict" else {
                throw error
            }
            return try await client.get(
                ownerUserID: context.ownerUserID,
                conversationID: sessionID
            )
        }
    }

    private func put(
        current: LocalAgentConversationRuntimeSettings,
        snapshot: LocalAgentModelConfigSnapshot,
        selectedThinkingLevel: String?,
        remoteConnectionID: String?,
        reasoningEnabled: Bool
    ) async throws -> LocalAgentConversationRuntimeSettings {
        try await client.put(.init(
            ownerUserID: current.ownerUserID,
            conversationID: current.conversationID,
            selectedModelConfigRef: snapshot.modelConfigRef,
            selectedModelConfigRevision: snapshot.modelConfigRevision,
            selectedThinkingLevel: selectedThinkingLevel,
            remoteConnectionID: remoteConnectionID,
            reasoningEnabled: reasoningEnabled,
            expectedVersion: current.version
        ))
    }

    private func snapshot(
        for settings: LocalAgentConversationRuntimeSettings
    ) throws -> LocalAgentModelConfigSnapshot {
        let context = try requireContext()
        guard let snapshot = context.modelSnapshots.first(where: {
            $0.modelConfigRef == settings.selectedModelConfigRef
                && $0.modelConfigRevision == settings.selectedModelConfigRevision
        }) else {
            throw NativeLocalAgentConversationRuntimeSettingsError.staleModelRevision
        }
        return snapshot
    }

    private func option(for modelID: String) throws -> ConversationModelOption {
        guard let option = try requireContext().modelOptions.first(where: { $0.id == modelID }) else {
            throw NativeLocalAgentConversationRuntimeSettingsError.unknownModel
        }
        return option
    }

    private func enabledLevel(
        current: LocalAgentConversationRuntimeSettings,
        snapshot: LocalAgentModelConfigSnapshot,
        option: ConversationModelOption
    ) -> String {
        if let selected = current.selectedThinkingLevel {
            let level = normalizedThinkingLevel(selected, provider: snapshot.provider)
            if level != "none", option.thinkingLevels.contains(level) {
                return level
            }
        }
        let configured = defaultLevel(for: snapshot)
        if configured != "none", option.thinkingLevels.contains(configured) {
            return configured
        }
        if option.thinkingLevels.contains("auto") { return "auto" }
        if option.thinkingLevels.contains("medium") { return "medium" }
        return option.thinkingLevels.first(where: { $0 != "none" }) ?? "none"
    }

    private func defaultLevel(for snapshot: LocalAgentModelConfigSnapshot) -> String {
        guard let level = snapshot.thinkingLevel else { return "none" }
        return normalizedThinkingLevel(level, provider: snapshot.provider)
    }

    private func normalizedThinkingLevel(_ level: String, provider: String) -> String {
        let level = level.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let provider = provider.trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased().replacingOccurrences(of: "-", with: "_")
        switch level {
        case "off", "disabled", "none":
            return "none"
        case "max", "xhigh":
            return provider == "deepseek" ? "max" : "xhigh"
        case "minimal" where provider == "openai_compatible" || provider == "compatible":
            return "low"
        default:
            return level
        }
    }

    private func domainSettings(
        from settings: LocalAgentConversationRuntimeSettings
    ) throws -> ConversationRuntimeSettings {
        let snapshot = try snapshot(for: settings)
        return .init(
            selectedModelID: settings.selectedModelConfigRef,
            selectedModelName: try option(for: snapshot.modelConfigRef).displayName,
            selectedThinkingLevel: settings.selectedThinkingLevel,
            remoteConnectionID: settings.remoteConnectionID,
            reasoningEnabled: settings.reasoningEnabled
        )
    }

    private func requireContext() throws -> Context {
        guard let context else {
            throw NativeLocalAgentConversationRuntimeSettingsError.notConfigured
        }
        return context
    }
}

public enum NativeLocalAgentConversationRuntimeSettingsError: LocalizedError {
    case notConfigured
    case unknownModel
    case staleModelRevision
    case unsupportedThinkingLevel

    public var errorDescription: String? {
        switch self {
        case .notConfigured:
            "Local Agent conversation settings are not configured."
        case .unknownModel:
            "The selected Local Agent model is unavailable."
        case .staleModelRevision:
            "The selected Local Agent model revision is no longer available."
        case .unsupportedThinkingLevel:
            "The selected reasoning level is not supported by this model."
        }
    }
}
