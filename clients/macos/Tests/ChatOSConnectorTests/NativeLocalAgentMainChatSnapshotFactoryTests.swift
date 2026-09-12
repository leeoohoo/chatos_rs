// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
@testable import ChatOSConnector
import Foundation
import Testing

struct NativeLocalAgentMainChatSnapshotFactoryTests {
    @Test("freezes exact Main Chat prompt, capability and safe project payloads")
    func buildsExactSnapshots() throws {
        let contact = LocalAgentContactRuntimeContext(
            agentID: "agent-design",
            name: "Visual Designer",
            description: "A careful design partner.",
            category: "design",
            roleDefinition: "Create polished visual systems.",
            skills: [
                LocalAgentContactSkill(
                    id: "visual-review",
                    name: "Visual review",
                    content: "Review hierarchy and typography."
                ),
            ],
            revision: "2026-09-12T00:00:00Z"
        )
        let project = LocalProjectRecord(
            id: "project-1",
            ownerUserID: "user-1",
            draft: LocalProjectDraft(
                name: "Landing page",
                description: "A restrained editorial storefront.",
                workspaceID: "workspace-1"
            ),
            revision: 7,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 2
        )

        let snapshots = try NativeLocalAgentMainChatSnapshotFactory().make(
            contact: contact,
            project: project
        )

        guard case let .object(prompt) = snapshots.prompt.payload,
              case let .object(capabilities) = snapshots.capabilities.payload,
              let projectSnapshot = snapshots.project,
              case let .object(projectPayload) = projectSnapshot.payload
        else {
            Issue.record("expected object snapshots")
            return
        }
        #expect(prompt.keys.sorted() == [
            "base_system_prompt",
            "contact_system_prompt",
            "prompt_revision",
            "skill_catalog_prompt",
        ])
        #expect(prompt["prompt_revision"] == .string(snapshots.prompt.revision))
        #expect(capabilities == [
            "snapshot_ref": .string("main-chat-capabilities-v1"),
            "allowed_tools": .array([.string("ask_user"), .string("create_local_task")]),
        ])
        #expect(projectPayload.keys.sorted() == [
            "design_context", "project_id", "project_name", "snapshot_revision",
        ])
        #expect(projectPayload["project_id"] == .string("project-1"))
        #expect(projectPayload["snapshot_revision"] == .string("7"))
        #expect(projectSnapshot.revision == "7")
        #expect(!String(describing: projectSnapshot.payload).contains("workspace-1"))
    }

    @Test("refuses local paths in prompts and project design context")
    func rejectsLocalAuthorityLeakage() throws {
        let unsafeProject = LocalProjectRecord(
            id: "project-1",
            ownerUserID: "user-1",
            draft: LocalProjectDraft(
                name: "Project",
                description: "Read /Users/example/private",
                workspaceID: "workspace-1"
            ),
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
        #expect(throws: NativeLocalAgentMainChatSnapshotError.unsafePromptContent) {
            _ = try NativeLocalAgentMainChatSnapshotFactory().make(
                contact: nil,
                project: unsafeProject
            )
        }
    }
}
