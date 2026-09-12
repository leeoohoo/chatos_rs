// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public enum NativeLocalAgentMainChatSnapshotError: Error, Equatable, Sendable {
    case invalidContactContext
    case unsafePromptContent
    case projectMismatch
}

extension NativeLocalAgentMainChatSnapshotError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .invalidContactContext: "联系人 Agent 运行上下文无效"
        case .unsafePromptContent: "主聊天提示中包含不能发送给模型的本地路径"
        case .projectMismatch: "本地 Agent 项目快照与当前会话不一致"
        }
    }
}

public struct NativeLocalAgentMainChatSnapshots: Sendable, Equatable {
    public var prompt: LocalAgentFrozenSnapshot
    public var capabilities: LocalAgentFrozenSnapshot
    public var project: LocalAgentFrozenSnapshot?
}

public struct NativeLocalAgentMainChatSnapshotFactory: Sendable {
    public static let basePrompt = """
    You are ChatOS's primary collaboration agent. Focus first on understanding and improving the user's intended outcome. Give a direct answer when no external action is needed. Ask the user only when a missing decision materially changes the result; do not request confirmation for reversible, read-only, or already-authorized work. For UI and website design, work visually and incrementally: establish art direction and composition, use visual references when available, refine one bounded region at a time, and prioritize a polished interface over speculative interaction behavior. Never claim that files, code, deployments, or external systems changed unless a completed local Task result proves it.
    """

    public init() {}

    public func make(
        contact: LocalAgentContactRuntimeContext?,
        project: LocalProjectRecord?
    ) throws -> NativeLocalAgentMainChatSnapshots {
        let contactPrompt = try contact.map(makeContactPrompt)
        let skillPrompt = try contact.flatMap(makeSkillCatalogPrompt)
        for value in [Self.basePrompt, contactPrompt, skillPrompt].compactMap({ $0 })
            where containsLocalPath(value)
        {
            throw NativeLocalAgentMainChatSnapshotError.unsafePromptContent
        }

        let promptSource: LocalAgentJSONValue = .object([
            "base_system_prompt": .string(Self.basePrompt),
            "contact_system_prompt": contactPrompt.map(LocalAgentJSONValue.string) ?? .null,
            "skill_catalog_prompt": skillPrompt.map(LocalAgentJSONValue.string) ?? .null,
        ])
        let promptSourceDigest = try NativePluginHash.localAgentCanonicalSHA256(promptSource)
        let promptRevision = "main-chat-native-v1-\(promptSourceDigest.prefix(16))"
        let prompt = try NativeLocalAgentSnapshotBuilder.make(
            snapshotID: "main-chat-prompt",
            revision: promptRevision,
            payload: .object([
                "prompt_revision": .string(promptRevision),
                "base_system_prompt": .string(Self.basePrompt),
                "contact_system_prompt": contactPrompt.map(LocalAgentJSONValue.string) ?? .null,
                "skill_catalog_prompt": skillPrompt.map(LocalAgentJSONValue.string) ?? .null,
            ])
        )

        let capabilityID = "main-chat-capabilities-v1"
        let capabilities = try NativeLocalAgentSnapshotBuilder.make(
            snapshotID: capabilityID,
            revision: "1",
            payload: .object([
                "snapshot_ref": .string(capabilityID),
                "allowed_tools": .array([
                    .string("ask_user"),
                    .string("create_local_task"),
                ]),
            ])
        )
        return NativeLocalAgentMainChatSnapshots(
            prompt: prompt,
            capabilities: capabilities,
            project: try project.map(makeProjectSnapshot)
        )
    }

    private func makeContactPrompt(_ contact: LocalAgentContactRuntimeContext) throws -> String {
        guard validIdentity(contact.agentID), validText(contact.name),
              validText(contact.roleDefinition), validIdentity(contact.revision),
              [contact.description, contact.category].compactMap({ $0 }).allSatisfy(validText)
        else {
            throw NativeLocalAgentMainChatSnapshotError.invalidContactContext
        }
        return [
            "Contact Agent: \(contact.name)",
            contact.description.map { "Description: \($0)" },
            contact.category.map { "Category: \($0)" },
            "Role: \(contact.roleDefinition)",
        ].compactMap { $0 }.joined(separator: "\n")
    }

    private func makeSkillCatalogPrompt(
        _ contact: LocalAgentContactRuntimeContext
    ) throws -> String? {
        guard !contact.skills.isEmpty else { return nil }
        var seen = Set<String>()
        let skills = try contact.skills.sorted { $0.id < $1.id }.map { skill in
            guard validIdentity(skill.id), validText(skill.name), validText(skill.content),
                  seen.insert(skill.id).inserted
            else {
                throw NativeLocalAgentMainChatSnapshotError.invalidContactContext
            }
            return "Skill \(skill.id) — \(skill.name):\n\(skill.content)"
        }
        return "Frozen Skill Catalog:\n\n" + skills.joined(separator: "\n\n")
    }

    private func makeProjectSnapshot(_ project: LocalProjectRecord) throws
        -> LocalAgentFrozenSnapshot
    {
        try project.validate()
        guard project.status == .active else {
            throw NativeLocalAgentMainChatSnapshotError.projectMismatch
        }
        if containsLocalPath(project.draft.description) {
            throw NativeLocalAgentMainChatSnapshotError.unsafePromptContent
        }
        let revision = String(project.revision)
        return try NativeLocalAgentSnapshotBuilder.make(
            snapshotID: "main-chat-project-\(project.id)",
            revision: revision,
            payload: .object([
                "project_id": .string(project.id),
                "snapshot_revision": .string(revision),
                "project_name": .string(project.draft.name),
                "design_context": .object([
                    "description": .string(project.draft.description),
                ]),
            ])
        )
    }

    private func validIdentity(_ value: String) -> Bool {
        !value.isEmpty
            && value.utf8.count <= 512
            && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
            && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }

    private func validText(_ value: String) -> Bool {
        !value.isEmpty
            && value.utf8.count <= 256 * 1_024
            && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
            && !value.contains("\0")
    }

    private func containsLocalPath(_ value: String) -> Bool {
        value.contains("file://")
            || value.contains("/Users/")
            || value.contains("/Volumes/")
            || value.contains("/home/")
            || value.range(of: #"[A-Za-z]:[\\/]"#, options: .regularExpression) != nil
    }
}
