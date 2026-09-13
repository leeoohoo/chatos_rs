// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation
import Testing

@Suite("Shared Local Agent protocol v25 fixtures")
struct LocalAgentProtocolV25FixtureTests {
    private struct Request: Encodable {
        let protocolVersion: UInt32
        let requestID: String
        let ownerUserID: String
        let command: LocalAgentCommand
    }

    @Test("encodes the shared retry Task request without a native schema fork")
    func retryTaskRequest() throws {
        let request = Request(
            protocolVersion: localAgentProtocolVersion,
            requestID: "request-retry-task-1",
            ownerUserID: "user-1",
            command: .retryTask(LocalAgentRetryTask(
                taskID: "task-1",
                expectedRunID: "task-run-1",
                instruction: "Preserve the approved visual hierarchy."
            ))
        )
        let encoder = LocalAgentProtocolJSON.encoder()
        let encoded = try JSONSerialization.jsonObject(with: encoder.encode(request)) as? NSDictionary
        let fixture = try JSONSerialization.jsonObject(
            with: Data(contentsOf: fixtureURL("retry_task_request.json"))
        ) as? NSDictionary

        #expect(localAgentProtocolVersion == 25)
        #expect(encoded == fixture)
    }

    @Test("encodes Clipboard metadata without transporting payload bytes")
    func clipboardMetadata() throws {
        let request = Request(
            protocolVersion: localAgentProtocolVersion,
            requestID: "request-store-clipboard-1",
            ownerUserID: "user-1",
            command: .storeClipboard(
                entryID: "00000000-0000-4000-8000-000000000001",
                draft: .init(
                    kind: .text,
                    mimeType: "text/plain",
                    contentHash: "sha256:" + String(repeating: "a", count: 64),
                    textPreview: "Clipboard preview",
                    sourceBundleID: "com.example.editor",
                    payloadReference: "Payloads/c6c289e49e9c05b2145860387b73bcb18df43fb09a1e4a4a9713c76c88bb541b/00000000-0000-4000-8000-000000000001.txt",
                    byteCount: 17,
                    pasteboardType: nil
                )
            )
        )
        let encodedData = try LocalAgentProtocolJSON.encoder().encode(request)
        let encoded = try JSONSerialization.jsonObject(with: encodedData) as? NSDictionary
        let fixtureData = try Data(contentsOf: fixtureURL("clipboard_store_request.json"))
        let fixture = try JSONSerialization.jsonObject(with: fixtureData) as? NSDictionary
        #expect(encoded == fixture)
        #expect(String(decoding: encodedData, as: UTF8.self).contains("Clipboard preview"))
        #expect(!String(decoding: encodedData, as: UTF8.self).contains("hello clipboard"))

        let reply = try LocalAgentProtocolJSON.decoder().decode(
            LocalAgentIPCReply.self,
            from: Data(contentsOf: fixtureURL("clipboard_mutation_response.json"))
        )
        guard case let .clipboardMutation(result) = reply.response else {
            Issue.record("Expected a Clipboard mutation response")
            return
        }
        #expect(result.entry?.ownerUserID == "user-1")
        #expect(result.entry?.revision == 1)
        #expect(result.discardedPayloadReferences.isEmpty)
    }

    @Test("encodes Media history metadata without transporting payload bytes")
    func mediaMetadata() throws {
        let request = Request(
            protocolVersion: localAgentProtocolVersion,
            requestID: "request-put-media-1",
            ownerUserID: "user-1",
            command: .putMedia(
                recordID: "00000000-0000-4000-8000-000000000010",
                expectedRevision: nil,
                draft: .init(
                    projectID: "project-1",
                    kind: .image,
                    status: .completed,
                    prompt: "A polished product hero",
                    modelName: "gpt-image-2",
                    generatedAt: "2026-09-09T00:00:00Z",
                    assets: [.init(
                        assetID: "asset-1",
                        mimeType: "image/png",
                        payloadReference: "Payloads/c6c289e49e9c05b2145860387b73bcb18df43fb09a1e4a4a9713c76c88bb541b/00000000-0000-4000-8000-000000000010/asset-1.png",
                        contentHash: "sha256:" + String(repeating: "a", count: 64),
                        byteCount: 1024,
                        revisedPrompt: "A polished product hero on a soft blue background"
                    )]
                )
            )
        )
        let encoded = try JSONSerialization.jsonObject(
            with: LocalAgentProtocolJSON.encoder().encode(request)
        ) as? NSDictionary
        let fixture = try JSONSerialization.jsonObject(
            with: Data(contentsOf: fixtureURL("media_put_request.json"))
        ) as? NSDictionary
        #expect(encoded == fixture)

        let reply = try LocalAgentProtocolJSON.decoder().decode(
            LocalAgentIPCReply.self,
            from: Data(contentsOf: fixtureURL("media_mutation_response.json"))
        )
        guard case let .mediaMutation(result) = reply.response else {
            Issue.record("Expected a Media mutation response")
            return
        }
        #expect(result.record?.ownerUserID == "user-1")
        #expect(result.record?.draft.projectID == "project-1")
        #expect(result.record?.draft.status == .completed)
        #expect(result.discardedPayloadReferences.isEmpty)
    }

    @Test("encodes and decodes owner-scoped Story state")
    func storyState() throws {
        let projectID = "00000000-0000-4000-8000-000000000020"
        let request = Request(
            protocolVersion: localAgentProtocolVersion,
            requestID: "request-put-story-1",
            ownerUserID: "user-1",
            command: .putStory(
                recordID: "project:\(projectID)",
                expectedRevision: nil,
                draft: .init(
                    projectID: projectID,
                    kind: .project,
                    status: "draft",
                    state: .object([
                        "id": .string(projectID),
                        "title": .string("Visual story"),
                    ])
                )
            )
        )
        let encoded = try JSONSerialization.jsonObject(
            with: LocalAgentProtocolJSON.encoder().encode(request)
        ) as? NSDictionary
        let fixture = try JSONSerialization.jsonObject(
            with: Data(contentsOf: fixtureURL("story_put_request.json"))
        ) as? NSDictionary
        #expect(encoded == fixture)

        let reply = try LocalAgentProtocolJSON.decoder().decode(
            LocalAgentIPCReply.self,
            from: Data(contentsOf: fixtureURL("story_records_response.json"))
        )
        guard case let .storyRecords(records, nextCursor) = reply.response else {
            Issue.record("Expected a Story records response")
            return
        }
        #expect(reply.protocolVersion == localAgentProtocolVersion)
        #expect(records.first?.ownerUserID == "user-1")
        #expect(records.first?.draft.projectID == projectID)
        #expect(records.first?.draft.kind == .project)
        #expect(nextCursor == nil)
    }

    @Test("encodes and decodes owner-scoped Notepad records")
    func notepadState() throws {
        let recordID = "note:00000000-0000-4000-8000-000000000021"
        let request = Request(
            protocolVersion: localAgentProtocolVersion,
            requestID: "request-notepad-put-1",
            ownerUserID: "user-1",
            command: .putNotepad(
                recordID: recordID,
                expectedRevision: nil,
                draft: .init(
                    kind: .note,
                    folder: "design/research",
                    title: "Visual direction",
                    content: "Use a cinematic layout.",
                    tags: ["design", "reference"]
                )
            )
        )
        let encoded = try JSONSerialization.jsonObject(
            with: LocalAgentProtocolJSON.encoder().encode(request)
        ) as? NSDictionary
        let fixture = try JSONSerialization.jsonObject(
            with: Data(contentsOf: fixtureURL("notepad_put_request.json"))
        ) as? NSDictionary
        #expect(encoded == fixture)

        let reply = try LocalAgentProtocolJSON.decoder().decode(
            LocalAgentIPCReply.self,
            from: Data(contentsOf: fixtureURL("notepad_records_response.json"))
        )
        guard case let .notepadRecords(records, nextCursor) = reply.response else {
            Issue.record("Expected a Notepad records response")
            return
        }
        #expect(records.first?.recordID == recordID)
        #expect(records.first?.ownerUserID == "user-1")
        #expect(records.first?.draft.kind == .note)
        #expect(nextCursor == nil)
    }

    @Test("encodes and decodes owner-scoped Client Settings")
    func clientSettingState() throws {
        let value: LocalAgentJSONValue = .object([
            "projects": .object([
                "project-1": .object([
                    "defaultTargetID": .string("target-1"),
                    "selectedToolchains": .object([:]),
                    "customToolchains": .object([:]),
                    "environmentVariables": .object([
                        "APP_ENV": .string("development"),
                    ]),
                ]),
            ]),
        ])
        let request = Request(
            protocolVersion: localAgentProtocolVersion,
            requestID: "request-client-setting-put-1",
            ownerUserID: "user-1",
            command: .putClientSetting(
                key: "project_run.preferences",
                expectedRevision: nil,
                value: value
            )
        )
        let encoded = try JSONSerialization.jsonObject(
            with: LocalAgentProtocolJSON.encoder().encode(request)
        ) as? NSDictionary
        let fixture = try JSONSerialization.jsonObject(
            with: Data(contentsOf: fixtureURL("client_setting_put_request.json"))
        ) as? NSDictionary
        #expect(encoded == fixture)

        let reply = try LocalAgentProtocolJSON.decoder().decode(
            LocalAgentIPCReply.self,
            from: Data(contentsOf: fixtureURL("client_setting_response.json"))
        )
        guard case let .clientSetting(setting) = reply.response else {
            Issue.record("Expected a Client Setting response")
            return
        }
        #expect(setting.key == "project_run.preferences")
        #expect(setting.ownerUserID == "user-1")
        #expect(setting.value == value)
        #expect(setting.revision == 3)
    }

    @Test("encodes and decodes owner-scoped Terminal History")
    func terminalHistoryState() throws {
        let request = Request(
            protocolVersion: localAgentProtocolVersion,
            requestID: "request-terminal-history-1",
            ownerUserID: "user-1",
            command: .appendTerminalHistory(
                recordID: "terminal:2026-09-14T10:00:00Z:1",
                draft: .init(
                    projectID: "project-1",
                    terminalSessionID: "native-terminal",
                    command: "cargo test",
                    exitCode: 0,
                    state: .object([
                        "source": .string("native-terminal"),
                        "workspace_alias": .string("workspace"),
                        "cwd": .string("/workspace"),
                        "display": .string("cargo test"),
                        "status": .string("completed"),
                        "stdout_preview": .string("ok"),
                        "stderr_preview": .null,
                        "error": .null,
                        "started_at": .string("2026-09-14T10:00:00Z"),
                    ])
                )
            )
        )
        let encoded = try JSONSerialization.jsonObject(
            with: LocalAgentProtocolJSON.encoder().encode(request)
        ) as? NSDictionary
        let fixture = try JSONSerialization.jsonObject(
            with: Data(contentsOf: fixtureURL("terminal_history_append_request.json"))
        ) as? NSDictionary
        #expect(encoded == fixture)

        let reply = try LocalAgentProtocolJSON.decoder().decode(
            LocalAgentIPCReply.self,
            from: Data(contentsOf: fixtureURL("terminal_history_records_response.json"))
        )
        guard case let .terminalHistoryRecords(records, nextCursor) = reply.response else {
            Issue.record("Expected Terminal History records")
            return
        }
        #expect(records.first?.ownerUserID == "user-1")
        #expect(records.first?.draft.terminalSessionID == "native-terminal")
        #expect(records.first?.draft.exitCode == 0)
        #expect(nextCursor == nil)
    }

    @Test("encodes and decodes owner-scoped Project CRUD")
    func projectCRUD() throws {
        let request = Request(
            protocolVersion: localAgentProtocolVersion,
            requestID: "request-create-project-1",
            ownerUserID: "user-1",
            command: .createProject(
                projectID: "project-1",
                draft: .init(
                    name: "Website Studio",
                    description: "Visual website design",
                    workspaceID: "workspace-1",
                    relativeRoot: "apps/studio"
                )
            )
        )
        let encoded = try JSONSerialization.jsonObject(
            with: LocalAgentProtocolJSON.encoder().encode(request)
        ) as? NSDictionary
        let fixture = try JSONSerialization.jsonObject(
            with: Data(contentsOf: fixtureURL("project_create_request.json"))
        ) as? NSDictionary
        #expect(encoded == fixture)

        let reply = try LocalAgentProtocolJSON.decoder().decode(
            LocalAgentIPCReply.self,
            from: Data(contentsOf: fixtureURL("project_response.json"))
        )
        guard case let .project(project) = reply.response else {
            Issue.record("Expected a Project response")
            return
        }
        #expect(project.ownerUserID == "user-1")
        #expect(project.projectID == "project-1")
        #expect(project.revision == 1)
    }

    @Test("encodes exact Run-bound tool approval from the shared contract")
    func toolApprovalRequest() throws {
        let request = Request(
            protocolVersion: localAgentProtocolVersion,
            requestID: "request-tool-approval-1",
            ownerUserID: "user-1",
            command: .decideToolApproval(
                runID: "run-1",
                invocationID: "invocation-1",
                decision: .reject,
                reason: "Rejected by the local user"
            )
        )
        let encoder = LocalAgentProtocolJSON.encoder()
        let encoded = try JSONSerialization.jsonObject(with: encoder.encode(request)) as? NSDictionary
        let fixture = try JSONSerialization.jsonObject(
            with: Data(contentsOf: fixtureURL("tool_approval_request.json"))
        ) as? NSDictionary

        #expect(encoded == fixture)
    }

    @Test("encodes Run control against the exact observed version")
    func runControlRequest() throws {
        let request = Request(
            protocolVersion: localAgentProtocolVersion,
            requestID: "request-pause-run-1",
            ownerUserID: "user-1",
            command: .pauseRun(runID: "run-1", expectedVersion: 7)
        )
        let encoder = LocalAgentProtocolJSON.encoder()
        let encoded = try JSONSerialization.jsonObject(with: encoder.encode(request)) as? NSDictionary
        let fixture = try JSONSerialization.jsonObject(
            with: Data(contentsOf: fixtureURL("run_control_request.json"))
        ) as? NSDictionary

        #expect(encoded == fixture)
    }

    @Test("decodes current and historical Runs from the shared Task snapshot")
    func taskSnapshotResponse() throws {
        let decoder = LocalAgentProtocolJSON.decoder()
        let reply = try decoder.decode(
            LocalAgentIPCReply.self,
            from: Data(contentsOf: fixtureURL("task_snapshot_response.json"))
        )
        guard case let .task(task) = reply.response else {
            Issue.record("Expected the shared fixture to contain a Task response")
            return
        }

        #expect(reply.protocolVersion == localAgentProtocolVersion)
        #expect(task.currentRunID == "task-run-2")
        #expect(task.runIDs == ["task-run-1", "task-run-2"])
        #expect(task.projectID == "project-1")
    }

    @Test("decodes shared Task Graph and Run detail projections")
    func taskProjectionResponses() throws {
        let decoder = LocalAgentProtocolJSON.decoder()
        let graphReply = try decoder.decode(
            LocalAgentIPCReply.self,
            from: Data(contentsOf: fixtureURL("task_graph_response.json"))
        )
        guard case let .taskGraph(graph) = graphReply.response else {
            Issue.record("Expected a Task Graph response")
            return
        }
        #expect(graph.rootTaskIDs == ["task-1"])
        #expect(graph.nodes.first?.task.task.projectID == "project-1")

        let detailReply = try decoder.decode(
            LocalAgentIPCReply.self,
            from: Data(contentsOf: fixtureURL("task_run_detail_response.json"))
        )
        guard case let .taskRunDetail(detail) = detailReply.response else {
            Issue.record("Expected a Task Run detail response")
            return
        }
        #expect(detail.task.initialRunID == "task-run-1")
        #expect(detail.run.run.runID == "task-run-2")
        #expect(detail.run.resultSummary == "Design implemented")
        #expect(detail.eventsTotal == 1)

        let runDetailReply = try decoder.decode(
            LocalAgentIPCReply.self,
            from: Data(contentsOf: fixtureURL("run_detail_response.json"))
        )
        guard case let .runDetail(runDetail) = runDetailReply.response else {
            Issue.record("Expected a generic Run detail response")
            return
        }
        #expect(runDetail.run.runID == "run-1")
        #expect(runDetail.events.first?.eventType == "message_assistant_reasoning")
        #expect(runDetail.snapshotEventSequence == 42)
    }

    @Test("decodes Run-bound Memory Sync status")
    func memorySyncEventResponse() throws {
        let reply = try LocalAgentProtocolJSON.decoder().decode(
            LocalAgentIPCReply.self,
            from: Data(contentsOf: fixtureURL("memory_sync_event_response.json"))
        )
        guard case let .events(events, nextSequence, hasMore) = reply.response,
              case let .memorySync(status) = events.first?.event
        else {
            Issue.record("Expected a Memory Sync event page")
            return
        }

        #expect(status.runID == "run-1")
        #expect(status.pendingCount == 2)
        #expect(status.failedCount == 1)
        #expect(nextSequence == 43)
        #expect(!hasMore)
    }

    @Test("encodes and decodes owner-scoped Approval History")
    func approvalHistoryState() throws {
        let request = Request(
            protocolVersion: localAgentProtocolVersion,
            requestID: "request-approval-history-1",
            ownerUserID: "user-1",
            command: .appendApprovalHistory(
                recordID: "approval-history-1",
                draft: .init(
                    command: "git push origin main",
                    cwd: "/workspace/project",
                    source: "native-terminal",
                    mode: "request_approval",
                    decision: "approved",
                    risk: "high",
                    reason: "Approved by the user"
                )
            )
        )
        let encoded = try JSONSerialization.jsonObject(
            with: LocalAgentProtocolJSON.encoder().encode(request)
        ) as? NSDictionary
        let fixture = try JSONSerialization.jsonObject(
            with: Data(contentsOf: fixtureURL("approval_history_append_request.json"))
        ) as? NSDictionary
        #expect(encoded == fixture)

        let reply = try LocalAgentProtocolJSON.decoder().decode(
            LocalAgentIPCReply.self,
            from: Data(contentsOf: fixtureURL("approval_history_records_response.json"))
        )
        guard case let .approvalHistoryRecords(records, nextCursor) = reply.response else {
            Issue.record("Expected an Approval History records response")
            return
        }
        #expect(records.first?.recordID == "approval-history-1")
        #expect(records.first?.ownerUserID == "user-1")
        #expect(records.first?.draft.decision == "approved")
        #expect(nextCursor == nil)
    }

    @Test("encodes and decodes owner-scoped installed Plugin state")
    func installedPluginState() throws {
        let installation: LocalAgentJSONValue = .object([
            "pluginID": .string("plugin-1"),
            "releaseID": .string("release-1"),
            "version": .string("1.2.3"),
            "artifactSHA256": .string(String(repeating: "a", count: 64)),
            "installationPath": .string("/Applications/ChatOS/Plugins/plugin-1/1.2.3"),
            "installedAt": .string("2026-09-14T02:00:00Z"),
        ])
        let request = Request(
            protocolVersion: localAgentProtocolVersion,
            requestID: "request-installed-plugin-put-1",
            ownerUserID: "user-1",
            command: .putInstalledPlugin(
                expectedRevision: nil,
                draft: .init(
                    pluginID: "plugin-1",
                    release: "release-1",
                    enabled: true,
                    installation: installation
                )
            )
        )
        let encoded = try JSONSerialization.jsonObject(
            with: LocalAgentProtocolJSON.encoder().encode(request)
        ) as? NSDictionary
        let fixture = try JSONSerialization.jsonObject(
            with: Data(contentsOf: fixtureURL("installed_plugin_put_request.json"))
        ) as? NSDictionary
        #expect(encoded == fixture)

        let reply = try LocalAgentProtocolJSON.decoder().decode(
            LocalAgentIPCReply.self,
            from: Data(contentsOf: fixtureURL("installed_plugin_records_response.json"))
        )
        guard case let .installedPluginRecords(records, nextCursor) = reply.response else {
            Issue.record("Expected installed Plugin records")
            return
        }
        #expect(records.first?.ownerUserID == "user-1")
        #expect(records.first?.draft.release == "release-1")
        #expect(records.first?.draft.enabled == false)
        #expect(nextCursor == nil)
    }

    private func fixtureURL(_ name: String) -> URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("shared/fixtures/local_agent/v25")
            .appendingPathComponent(name)
    }
}
