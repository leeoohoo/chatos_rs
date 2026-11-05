@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentMessageTaskGraphServiceTests: XCTestCase {
    func testExecutionInterruptionHasLocalizedReasonWithoutTreatingItAsModelOutput() {
        let reason = NativeLocalAgentMessageTaskGraphService.executionError(.object([
            "reason_code": .string("execution_interrupted"), "error": .string("internal English diagnostic")
        ]))
        XCTAssertEqual(reason, NSLocalizedString("任务执行意外中断，无法确认当前步骤的结果。请先检查执行记录，再决定是否重试，避免重复操作。", comment: ""))
        XCTAssertEqual(NativeLocalAgentMessageTaskGraphService.executionError(.object([
            "reason": .string("Missing required input")
        ])), "Missing required input")
        XCTAssertNil(NativeLocalAgentMessageTaskGraphService.executionError(.object([
            "content": .string("Normal final response")
        ])))
    }

    func testFollowupGateHasVisibleStatusWithoutChangingTerminalTaskFact() throws {
        for (status, review, expected) in [
            ("running", "pending", "reviewing_followup"),
            ("pending", "held", "waiting_confirmation"),
            ("running", "released", "running"),
            ("cancelled", "pending", "cancelled"),
        ] {
            let json: [String: Any] = [
                "graph_id": "graph", "owner_user_id": "user", "source_entity_type": "conversation_turn",
                "source_entity_id": "turn", "task_id": "task", "title": "Task",
                "model_config_ref": "model", "input": ["followup_review_state": review],
                "status": status, "version": 1, "created_at_unix_ms": 1, "updated_at_unix_ms": 2,
            ]
            let task = try JSONDecoder().decode(LocalAgentTaskRecord.self, from: JSONSerialization.data(withJSONObject: json))
            XCTAssertEqual(NativeLocalAgentMessageTaskGraphService.taskDisplayStatus(task), expected)
        }
    }

    func testMapsTurnTaskGraphAndCancelsWithCurrentVersion() async throws {
        let host = LocalTaskHostStub()
        let service = NativeLocalAgentMessageTaskGraphService(
            host: host,
            afterCancel: { runID in
                await host.recordRelease(runID)
            }
        )
        await service.configure(ownerUserID: "user-1")
        let lookup = MessageTaskLookup(
            sessionID: "conversation-1",
            turnID: "turn-1",
            sourceUserMessageID: "message-1"
        )

        let graph = try await service.fetchGraph(messageID: "message-1", lookup: lookup)
        XCTAssertEqual(graph.rootTaskIDs, ["task-1", "task-2", "task-3"])
        XCTAssertEqual(graph.nodes.map(\.task.id), ["task-1", "task-2", "task-3"])
        XCTAssertEqual(graph.nodes.map(\.depth), [0, 0, 0])
        XCTAssertEqual(graph.edges.first?.sourceID, "task-1")
        XCTAssertEqual(graph.edges.first?.targetID, "task-2")
        XCTAssertEqual(graph.edges.first?.kind, "prerequisite")
        XCTAssertEqual(graph.edges.last?.sourceID, "task-1")
        XCTAssertEqual(graph.edges.last?.targetID, "task-3")
        XCTAssertEqual(graph.edges.last?.kind, "context")
        XCTAssertEqual(graph.nodes[0].task.executionClientRef, "research")
        XCTAssertEqual(graph.nodes[2].task.dependencyContextRefs, ["research"])
        let recordedCommands = try await host.recordedCommands()
        let listCommand = try XCTUnwrap(recordedCommands.first)
        XCTAssertEqual(listCommand["type"], .string("get_message_task_graph"))
        XCTAssertEqual(listCommand["source_conversation_id"], .string("conversation-1"))
        XCTAssertEqual(listCommand["source_turn_id"], .string("turn-1"))

        let task = try await service.fetchTask(
            messageID: "message-1",
            taskID: "task-2",
            lookup: lookup
        )
        XCTAssertEqual(task.objective, "Ship locally")
        XCTAssertEqual(task.prerequisiteTaskIDs, ["task-1"])
        XCTAssertEqual(task.defaultModelConfig?.displayName, "gpt/gpt-6-sol")
        XCTAssertEqual(task.thinkingLevel, "high")
        XCTAssertEqual(task.lastRun?.resultSummary, "Completed locally")
        XCTAssertEqual(task.lastRun?.reportContent, "Completed locally")
        XCTAssertTrue(task.processLog?.contains("检查项目结构") == true)
        XCTAssertTrue(task.processLog?.contains("已确认入口和运行方式。") == true)
        XCTAssertFalse(task.processLog?.contains("list_dir") == true)
        XCTAssertFalse(task.processLog?.contains("private model output") == true)
        let toolUsageData = try XCTUnwrap(task.taskToolStateJSON?.data(using: .utf8))
        let toolUsage = try XCTUnwrap(
            JSONSerialization.jsonObject(with: toolUsageData) as? [String: Any]
        )
        let plugins = try XCTUnwrap(toolUsage["plugins"] as? [[String: Any]])
        XCTAssertEqual(plugins.first?["display_name"] as? String, "Document")
        XCTAssertEqual(plugins.first?["call_count"] as? Int, 1)
        let tools = try XCTUnwrap(toolUsage["tools"] as? [[String: Any]])
        XCTAssertTrue(tools.contains { $0["plugin_name"] as? String == "Document" })
        XCTAssertTrue(tools.contains { $0["name"] as? String == "list_dir" })

        let defaultThinkingTask = try await service.fetchTask(
            messageID: "message-1",
            taskID: "task-1",
            lookup: lookup
        )
        XCTAssertEqual(defaultThinkingTask.defaultModelConfig?.displayName, "gpt/gpt-6-sol")
        XCTAssertEqual(defaultThinkingTask.thinkingLevel, "medium")

        try await service.cancelTask(
            messageID: "message-1",
            taskID: "task-2",
            lookup: lookup,
            reason: "stop"
        )
        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("cancel_task"))
        XCTAssertEqual(command["task_id"], .string("task-2"))
        XCTAssertEqual(command["expected_version"], .number(2))
        let lifecycle = await host.recordedLifecycle()
        XCTAssertEqual(Array(lifecycle.suffix(2)), ["command:cancel_task", "release:run-task-2"])
    }

    func testGraphPropagatesBlockedDeliveryToDependentPresentationAndLoadsRunsConcurrently() async throws {
        let host = BlockedDependencyTaskHostStub()
        let service = NativeLocalAgentMessageTaskGraphService(host: host)
        await service.configure(ownerUserID: "user-1")

        let graph = try await service.fetchGraph(
            messageID: "message-1",
            lookup: .init(
                sessionID: "conversation-1",
                turnID: "turn-1",
                sourceUserMessageID: "message-1"
            )
        )

        XCTAssertEqual(graph.nodes[0].task.status, "succeeded")
        XCTAssertEqual(graph.nodes[0].task.lastRun?.status, "succeeded")
        XCTAssertEqual(graph.nodes[0].task.presentationStatus, .resultBlocked)
        XCTAssertEqual(graph.nodes[1].task.presentationStatus, .dependencyBlocked)
        XCTAssertEqual(graph.nodes[1].task.prerequisiteTasks.first?.resultStatus, "blocked")
        XCTAssertEqual(graph.nodes[1].task.prerequisiteTasks.first?.displayStatus, "result_blocked")
        let maximumConcurrentRunRequests = await host.maximumConcurrentRunRequests()
        XCTAssertGreaterThanOrEqual(maximumConcurrentRunRequests, 2)

        _ = try await service.fetchGraph(messageID: "message-1", lookup: .init(
            sessionID: "conversation-1",
            turnID: "turn-1",
            sourceUserMessageID: "message-1"
        ))
        let totalRunRequests = await host.totalRunRequests()
        XCTAssertEqual(totalRunRequests, 2,
                       "Terminal Tasks without a Run must cache the empty lookup across polls")
    }

    func testTransientRunLookupFailureIsRetriedBeforeCachingAnEmptyResult() async throws {
        let host = BlockedDependencyTaskHostStub(failFirstMissingRunRequest: true)
        let service = NativeLocalAgentMessageTaskGraphService(host: host)
        await service.configure(ownerUserID: "user-1")
        let lookup = MessageTaskLookup(
            sessionID: "conversation-1",
            turnID: "turn-1",
            sourceUserMessageID: "message-1"
        )

        _ = try await service.fetchGraph(messageID: "message-1", lookup: lookup)
        _ = try await service.fetchGraph(messageID: "message-1", lookup: lookup)
        _ = try await service.fetchGraph(messageID: "message-1", lookup: lookup)

        let totalRunRequests = await host.totalRunRequests()
        XCTAssertEqual(totalRunRequests, 3,
                       "A failed lookup must retry once; only a successful empty response is cached")
    }

    func testEmbeddedLatestRunsRemoveTaskGraphNPlusOneRequests() async throws {
        let host = BlockedDependencyTaskHostStub(embedLatestRuns: true)
        let service = NativeLocalAgentMessageTaskGraphService(host: host)
        await service.configure(ownerUserID: "user-1")
        let lookup = MessageTaskLookup(
            sessionID: "conversation-1",
            turnID: "turn-1",
            sourceUserMessageID: "message-1"
        )

        let graph = try await service.fetchGraph(messageID: "message-1", lookup: lookup)

        XCTAssertEqual(graph.nodes[0].task.presentationStatus, .resultBlocked)
        XCTAssertEqual(graph.nodes[1].task.presentationStatus, .dependencyBlocked)
        let totalRunRequests = await host.totalRunRequests()
        XCTAssertEqual(totalRunRequests, 0)
    }

    func testEmbeddedActiveRunsLoadPluginActivityInOneBatch() async throws {
        let host = BlockedDependencyTaskHostStub(
            embedLatestRuns: true,
            embedActiveRuns: true
        )
        let probe = PluginActivityBatchProbe()
        let service = NativeLocalAgentMessageTaskGraphService(
            host: host,
            pluginActivities: { runIDs in
                await probe.load(runIDs)
            }
        )
        await service.configure(ownerUserID: "user-1")

        let graph = try await service.fetchGraph(
            messageID: "message-1",
            lookup: .init(
                sessionID: "conversation-1",
                turnID: "turn-1",
                sourceUserMessageID: "message-1"
            )
        )

        let activityRequests = await probe.requests()
        let totalRunRequests = await host.totalRunRequests()
        XCTAssertEqual(activityRequests, [["run-task-1", "run-task-2"]])
        XCTAssertEqual(graph.nodes[0].task.lastRun?.activity?.operation, "batched-plugin")
        XCTAssertEqual(graph.nodes[1].task.lastRun?.activity?.operation, "batched-plugin")
        XCTAssertEqual(totalRunRequests, 0)
    }

    func testRetryPersistsAdditionalInstructionInLocalCommand() async throws {
        let host = LocalTaskHostStub()
        let service = NativeLocalAgentMessageTaskGraphService(
            host: host,
            beforeRetry: { runID in
                await host.recordRelease(runID)
            },
            afterRetry: {
                await host.recordWake()
            }
        )
        await service.configure(ownerUserID: "user-1")

        _ = try await service.retryRun(
            messageID: "message-1",
            runID: "run-task-2",
            lookup: .init(turnID: "turn-1"),
            instruction: "Use the local fallback"
        )

        let command = try await host.lastCommand()
        let lifecycle = await host.recordedLifecycle()
        XCTAssertEqual(command["type"], .string("retry_task"))
        XCTAssertEqual(command["retry_instruction"], .string("Use the local fallback"))
        XCTAssertEqual(
            Array(lifecycle.suffix(3)),
            ["release:run-task-2", "command:retry_task", "wake"]
        )
    }

    func testRestartRunningTaskReleasesOldPluginAfterDurableResetAndWakesScheduler() async throws {
        let host = LocalTaskHostStub()
        let service = NativeLocalAgentMessageTaskGraphService(
            host: host,
            afterRetry: {
                await host.recordWake()
            },
            afterCancel: { runID in
                await host.recordRelease(runID)
            }
        )
        await service.configure(ownerUserID: "user-1")

        let restarted = try await service.restartTask(
            messageID: "message-1",
            taskID: "task-2",
            lookup: MessageTaskLookup(
                sessionID: "conversation-1",
                turnID: "turn-1",
                sourceUserMessageID: "message-1"
            ),
            reason: "recover after verifying side effects"
        )

        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("restart_task"))
        XCTAssertEqual(command["task_id"], .string("task-2"))
        XCTAssertEqual(command["expected_version"], .number(2))
        XCTAssertEqual(command["reason"], .string("recover after verifying side effects"))
        XCTAssertEqual(restarted.taskID, "task-2")
        let lifecycle = await host.recordedLifecycle()
        XCTAssertEqual(
            Array(lifecycle.suffix(3)),
            ["command:restart_task", "release:run-task-2", "wake"]
        )
    }

    func testTaskClientRestartsWithCurrentVersionAndReason() async throws {
        let host = LocalTaskHostStub()
        let client = NativeLocalAgentTaskClient(host: host)

        _ = try await client.restart(
            ownerUserID: "user-1",
            taskID: "task-2",
            expectedVersion: 2,
            reason: "restart from the beginning"
        )

        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("restart_task"))
        XCTAssertEqual(command["owner_user_id"], .string("user-1"))
        XCTAssertEqual(command["task_id"], .string("task-2"))
        XCTAssertEqual(command["expected_version"], .number(2))
        XCTAssertEqual(command["reason"], .string("restart from the beginning"))
    }

    func testRunDetailsRequestNewestEventsWithLookAhead() async throws {
        let host = LocalTaskHostStub()
        let service = NativeLocalAgentMessageTaskGraphService(host: host)
        await service.configure(ownerUserID: "user-1")

        let detail = try await service.fetchRun(
            messageID: "message-1",
            runID: "run-task-2",
            lookup: .init(
                sessionID: "conversation-1",
                turnID: "turn-1",
                sourceUserMessageID: "message-1"
            ),
            includeEvents: true,
            eventLimit: 40,
            eventOffset: 0
        )

        XCTAssertEqual(detail.events.map(\.id), ["event-1"])
        let commands = try await host.recordedCommands()
        let eventCommand = try XCTUnwrap(commands.last(where: {
            $0["type"] == .string("list_events")
                && $0["newest_first"] == .bool(true)
        }))
        XCTAssertEqual(eventCommand["after_cursor"], .number(0))
        XCTAssertEqual(eventCommand["limit"], .number(41))
    }

    func testProcessLogHonorsReplaceClearAndSuccessfulInvocationsOnly() throws {
        let events = try JSONDecoder().decode(
            [LocalAgentEventRecord].self,
            from: JSONSerialization.data(withJSONObject: [
                processEvent(cursor: 1, operation: "append", content: "first"),
                processEvent(cursor: 2, operation: "replace", content: "replacement"),
                processEvent(
                    cursor: 3,
                    operation: "append",
                    content: "failed entry",
                    status: "failed"
                ),
                processEvent(cursor: 4, operation: "clear", content: NSNull()),
                processEvent(cursor: 5, operation: "append", content: "final milestone"),
            ])
        )

        let log = NativeLocalAgentMessageTaskGraphService.processLog(from: events)

        XCTAssertTrue(log?.contains("final milestone") == true)
        XCTAssertFalse(log?.contains("first") == true)
        XCTAssertFalse(log?.contains("replacement") == true)
        XCTAssertFalse(log?.contains("failed entry") == true)
    }

    func testAcceptanceProgressUsesLatestSuccessfulCheckPerCriterion() throws {
        let events = try JSONDecoder().decode(
            [LocalAgentEventRecord].self,
            from: JSONSerialization.data(withJSONObject: [
                processEvent(
                    cursor: 1,
                    operation: "append",
                    content: "checking",
                    acceptanceChecks: [[
                        "criterion_index": 0,
                        "status": "in_progress",
                        "evidence": "Running the focused test",
                    ]]
                ),
                processEvent(
                    cursor: 2,
                    operation: "append",
                    content: "verified",
                    acceptanceChecks: [[
                        "criterion_index": 0,
                        "status": "passed",
                        "evidence": "Focused test passed",
                    ], [
                        "criterion_index": 1,
                        "status": "blocked",
                        "evidence": "User must connect a real device",
                    ]]
                ),
                processEvent(
                    cursor: 3,
                    operation: "append",
                    content: "ignored",
                    status: "failed",
                    acceptanceChecks: [[
                        "criterion_index": 1,
                        "status": "passed",
                        "evidence": "Must not override successful history",
                    ]]
                ),
            ])
        )

        let progress = NativeLocalAgentMessageTaskGraphService.acceptanceProgress(from: events)

        XCTAssertEqual(progress.map(\.criterionIndex), [0, 1])
        XCTAssertEqual(progress.map(\.status), [.passed, .blocked])
        XCTAssertEqual(progress[0].evidence, "Focused test passed")
    }

    func testNeedsReviewErrorSurfacesLatestToolFailure() throws {
        let events = try JSONDecoder().decode(
            [LocalAgentEventRecord].self,
            from: JSONSerialization.data(withJSONObject: [
                invocationEvent(cursor: 1, status: "needs_review", error: "older failure"),
                invocationEvent(cursor: 2, status: "failed", error: "ordinary failure"),
                invocationEvent(
                    cursor: 3,
                    status: "needs_review",
                    error: "文本匹配数量不符合预期：期望 4，实际 3"
                ),
            ])
        )

        XCTAssertEqual(
            NativeLocalAgentMessageTaskGraphService.needsReviewError(from: events),
            "文本匹配数量不符合预期：期望 4，实际 3"
        )
    }

    func testRunDetailsFetchAndDisplayRunLevelFailure() async throws {
        let failure = "storage conflict: Memory record id was reused"
        let host = LocalTaskHostStub(runError: failure)
        let service = NativeLocalAgentMessageTaskGraphService(host: host)
        await service.configure(ownerUserID: "user-1")
        let detail = try await service.fetchRun(
            messageID: "message-1", runID: "run-task-2", lookup: .init(turnID: "turn-1"),
            includeEvents: true, eventLimit: 40, eventOffset: 0
        )
        XCTAssertEqual(detail.run.errorMessage, failure)
        XCTAssertEqual(detail.task.lastRun?.errorMessage, failure)
        XCTAssertEqual(detail.events.first?.message, failure)
        let commands = try await host.recordedCommands()
        XCTAssertTrue(commands.contains {
            $0["type"] == .string("list_events") && $0["event_type"] == .string("run_needs_review")
        })
    }

    func testNeedsReviewErrorSurfacesRunFailureWithNoFailedTools() throws {
        let events = try JSONDecoder().decode(
            [LocalAgentEventRecord].self,
            from: JSONSerialization.data(withJSONObject: [
                invocationEvent(cursor: 1, status: "succeeded", error: ""),
                [
                    "cursor": 2, "event_id": "run-failure", "run_id": "run-task-2",
                    "event_type": "run_needs_review",
                    "payload": [
                        "reason": "Local Agent profile step failed",
                        "detail": ["error": "storage conflict: Memory record id was reused"],
                    ],
                    "created_at_unix_ms": 2,
                ],
            ])
        )
        XCTAssertEqual(
            NativeLocalAgentMessageTaskGraphService.needsReviewError(from: events),
            "storage conflict: Memory record id was reused"
        )
    }

    func testNeedsReviewGenericRunReasonDoesNotHideConcreteToolError() throws {
        let events = try JSONDecoder().decode(
            [LocalAgentEventRecord].self,
            from: JSONSerialization.data(withJSONObject: [
                invocationEvent(cursor: 1, status: "needs_review", error: "specific tool error"),
                [
                    "cursor": 2, "event_id": "run-failure", "run_id": "run-task-2",
                    "event_type": "run_needs_review",
                    "payload": ["reason": "Tool needs review", "detail": ["error": "  "]],
                    "created_at_unix_ms": 2,
                ],
            ])
        )
        XCTAssertEqual(
            NativeLocalAgentMessageTaskGraphService.needsReviewError(from: events),
            "specific tool error"
        )
        XCTAssertEqual(
            NativeLocalAgentMessageTaskGraphService.needsReviewError(from: [events[1]]),
            "Tool needs review"
        )
    }

    private func processEvent(
        cursor: Int,
        operation: String,
        content: Any,
        status: String = "succeeded",
        acceptanceChecks: [[String: Any]] = []
    ) -> [String: Any] {
        [
            "cursor": cursor,
            "event_id": "event-\(cursor)",
            "run_id": "run-task-2",
            "event_type": "tool_batch_completed",
            "payload": [
                "invocations": [[
                    "tool_name": "task_run_process_record_process",
                    "status": status,
                    "arguments": [
                        "operation": operation,
                        "content": content,
                        "acceptance_checks": acceptanceChecks,
                    ],
                ]],
            ],
            "created_at_unix_ms": cursor,
        ]
    }

    private func invocationEvent(cursor: Int, status: String, error: String) -> [String: Any] {
        [
            "cursor": cursor,
            "event_id": "invocation-event-\(cursor)",
            "run_id": "run-task-2",
            "event_type": "tool_invocation_completed",
            "payload": ["status": status, "error": error],
            "created_at_unix_ms": cursor,
        ]
    }
}

private actor BlockedDependencyTaskHostStub: LocalAgentHostClientServicing {
    private var activeRunRequests = 0
    private var maximumRunRequests = 0
    private var runRequests = 0
    private let failFirstMissingRunRequest: Bool
    private let embedLatestRuns: Bool
    private let embedActiveRuns: Bool
    private var didFailMissingRunRequest = false

    init(
        failFirstMissingRunRequest: Bool = false,
        embedLatestRuns: Bool = false,
        embedActiveRuns: Bool = false
    ) {
        self.failFirstMissingRunRequest = failFirstMissingRunRequest
        self.embedLatestRuns = embedLatestRuns
        self.embedActiveRuns = embedActiveRuns
    }

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func maximumConcurrentRunRequests() -> Int { maximumRunRequests }
    func totalRunRequests() -> Int { runRequests }

    func request(command: Data) async throws -> Data {
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        switch object?["type"] as? String {
        case "get_message_task_graph":
            return try json([
                "type": "message_task_graph",
                "graph": [
                    "root_task_ids": ["task-1"],
                    "nodes": [
                        node(
                            task(id: "task-1", status: embedActiveRuns ? "running" : "succeeded"),
                            latestRun: embedActiveRuns
                                ? activeRun(taskID: "task-1") : blockedRun(),
                            depth: 0,
                            root: true
                        ),
                        node(
                            task(id: "task-2", status: embedActiveRuns ? "running" : "blocked"),
                            latestRun: embedActiveRuns
                                ? activeRun(taskID: "task-2") : nil,
                            depth: 1,
                            root: false
                        ),
                    ],
                    "edges": [[
                        "source_task_id": "task-1",
                        "target_task_id": "task-2",
                        "kind": "prerequisite",
                    ]],
                    "source_conversation_id": "conversation-1",
                    "source_turn_id": "turn-1",
                    "source_user_message_id": "message-1",
                ],
            ])
        case "get_task_runs":
            runRequests += 1
            activeRunRequests += 1
            maximumRunRequests = max(maximumRunRequests, activeRunRequests)
            try await Task.sleep(for: .milliseconds(30))
            activeRunRequests -= 1
            let taskID = object?["task_id"] as? String ?? ""
            if taskID == "task-2", failFirstMissingRunRequest, !didFailMissingRunRequest {
                didFailMissingRunRequest = true
                throw URLError(.networkConnectionLost)
            }
            return try json([
                "type": "task_runs",
                "task_id": taskID,
                "runs": taskID == "task-1" ? [blockedRun()] : [],
            ])
        default:
            throw CocoaError(.featureUnsupported)
        }
    }

    private func node(
        _ task: [String: Any],
        latestRun: [String: Any]?,
        depth: Int,
        root: Bool
    ) -> [String: Any] {
        var node: [String: Any] = [
            "task": task,
            "depth": depth,
            "is_root": root,
            "is_current_message": true,
        ]
        if embedLatestRuns {
            node["latest_run"] = latestRun ?? NSNull()
        }
        return node
    }

    private func task(id: String, status: String) -> [String: Any] {
        [
            "graph_id": "graph-1",
            "owner_user_id": "user-1",
            "source_entity_type": "conversation_turn",
            "source_entity_id": "turn-1",
            "task_id": id,
            "title": id == "task-1" ? "Create plan" : "Implement plan",
            "model_config_ref": "model-1",
            "input": ["objective": "Ship the plan"],
            "status": status,
            "active_run_id": NSNull(),
            "version": 1,
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 2,
        ]
    }

    private func blockedRun() -> [String: Any] {
        [
            "run_id": "run-task-1",
            "owner_user_id": "user-1",
            "owner_entity_type": "task",
            "owner_entity_id": "task-1",
            "profile_key": "task_execution",
            "capability_policy_revision": "policy-1",
            "input": [:],
            "status": "succeeded",
            "version": 1,
            "terminal_outcome": [
                "content": "Planning could not be saved.",
                "task_acceptance": [
                    "result_status": "blocked",
                    "reason": "The saved revision did not change.",
                ],
            ],
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 2,
        ]
    }

    private func activeRun(taskID: String) -> [String: Any] {
        [
            "run_id": "run-\(taskID)",
            "owner_entity_id": taskID,
            "status": "waiting_tool_result",
            "terminal_outcome": NSNull(),
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 2,
        ]
    }

    private func json(_ value: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: value)
    }
}

private actor PluginActivityBatchProbe {
    private var batches: [[String]] = []

    func load(_ runIDs: Set<String>) -> [String: MessageTaskRunActivity] {
        batches.append(runIDs.sorted())
        return Dictionary(uniqueKeysWithValues: runIDs.map { runID in
            (runID, MessageTaskRunActivity(kind: .waitingTool, operation: "batched-plugin"))
        })
    }

    func requests() -> [[String]] { batches }
}

private actor LocalTaskHostStub: LocalAgentHostClientServicing {
    private var commands: [Data] = []
    private var lifecycle: [String] = []
    private let runError: String?

    init(runError: String? = nil) {
        self.runError = runError
    }

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        lifecycle.append("command:\(object?["type"] as? String ?? "unknown")")
        switch object?["type"] as? String {
        case "get_message_task_graph":
            return try json(["type": "message_task_graph", "graph": messageGraph()])
        case "list_task_graphs":
            return try json([
                "type": "task_graphs",
                "page": [
                    "graphs": [[
                        "graph_id": "graph-1",
                        "source_entity_type": "conversation_turn",
                        "source_entity_id": "turn-1",
                        "updated_at_unix_ms": 3,
                    ]],
                    "next_before_updated_at_unix_ms": NSNull(),
                    "next_before_graph_id": NSNull(),
                ],
            ])
        case "get_task_graph", "cancel_task", "restart_task":
            return try json(["type": "task_graph", "graph": graph()])
        case "retry_task":
            return try json(["type": "task_graph", "graph": graph()])
        case "get_run":
            return try json([
                "type": "run",
                "run": run(),
            ])
        case "get_model_config_snapshot":
            return try json([
                "type": "model_config_snapshot",
                "snapshot": [
                    "owner_user_id": "user-1",
                    "model_config_ref": "model-1",
                    "model_config_revision": "revision-1",
                    "credential_ref": "env:MODEL_1_API_KEY",
                    "base_url": "https://api.example.test/v1",
                    "model": "gpt-6-sol",
                    "provider": "gpt",
                    "supports_responses": true,
                    "thinking_level": "medium",
                    "include_prompt_cache_retention": false,
                ],
            ])
        case "get_task_runs":
            let taskID = object?["task_id"] as? String ?? "task-1"
            return try json([
                "type": "task_runs",
                "task_id": taskID,
                "runs": taskID == "task-2" ? [run()] : [],
            ])
        case "get_run_capability_disclosures":
            return try json([
                "type": "run_capability_disclosures",
                "run_id": "run-task-2",
                "disclosures": [[
                    "plugin_option": "plugin_1",
                    "tool_option": "tool_1",
                    "tool_name": "document_create",
                    "provider_id": "plugin:plugin-document",
                    "schema_sha256": String(repeating: "a", count: 64),
                ]],
            ])
        case "list_events":
            if let runError {
                let filter = object?["event_type"] as? String
                let events = filter == nil || filter == "run_needs_review"
                    ? [event(cursor: 2, type: "run_needs_review", payload: [
                        "reason": "Local Agent profile step failed",
                        "detail": ["error": runError],
                    ])] : []
                return try json(["type": "events", "events": events, "next_cursor": 2])
            }
            let filter = object?["event_type"] as? String
            if filter == "tool_batch_requested" {
                return try json([
                    "type": "events",
                    "events": [
                        event(
                            cursor: 1,
                            type: "tool_batch_requested",
                            payload: [
                                "tool_calls": [
                                    [
                                        "call_id": "call-plugin",
                                        "tool_name": "capability_invoke",
                                        "arguments": [
                                            "plugin_option": "plugin_1",
                                            "tool_option": "tool_1",
                                            "arguments": [:],
                                        ],
                                    ],
                                    [
                                        "call_id": "call-list",
                                        "tool_name": "list_dir",
                                        "arguments": ["path": "."],
                                    ],
                                ],
                            ]
                        ),
                    ],
                    "next_cursor": 1,
                ])
            }
            return try json([
                "type": "events",
                "events": [
                    event(
                        cursor: 1,
                        type: "tool_batch_completed",
                        payload: [
                            "type": "tool_results",
                            "invocations": [
                                [
                                    "call_id": "call-list",
                                    "tool_name": "list_dir",
                                    "execution_route": "platform",
                                    "status": "succeeded",
                                    "arguments": ["path": "."],
                                ],
                                [
                                    "call_id": "call-plugin",
                                    "tool_name": "capability_invoke",
                                    "execution_route": "platform",
                                    "status": "succeeded",
                                ],
                                [
                                    "tool_name": "task_run_process_record_process",
                                    "status": "succeeded",
                                    "arguments": [
                                        "operation": "append",
                                        "heading": "检查项目结构",
                                        "content": "已确认入口和运行方式。",
                                    ],
                                ],
                            ],
                        ]
                    ),
                ],
                "next_cursor": 1,
            ])
        default:
            throw CocoaError(.featureUnsupported)
        }
    }

    func lastCommand() throws -> [String: LocalAgentJSONValue] {
        guard let data = commands.last else { throw CocoaError(.fileNoSuchFile) }
        let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: data)
        guard case let .object(object) = value else { throw CocoaError(.fileReadCorruptFile) }
        return object
    }

    func recordedCommands() throws -> [[String: LocalAgentJSONValue]] {
        try commands.map { data in
            let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: data)
            guard case let .object(object) = value else {
                throw CocoaError(.fileReadCorruptFile)
            }
            return object
        }
    }

    func recordRelease(_ runID: String) {
        lifecycle.append("release:\(runID)")
    }

    func recordWake() {
        lifecycle.append("wake")
    }

    func recordedLifecycle() -> [String] {
        lifecycle
    }

    private func graph() -> [String: Any] {
        [
            "graph_id": "graph-1",
            "owner_user_id": "user-1",
            "source_entity_type": "conversation_turn",
            "source_entity_id": "turn-1",
            "status": "running",
            "tasks": [
                task("task-1", version: 1, clientRef: "research"),
                task("task-2", version: 2, clientRef: "implement"),
                task(
                    "task-3",
                    version: 1,
                    clientRef: "review",
                    contextRefs: ["research"]
                ),
            ],
            "dependencies": [[
                "task_id": "task-2",
                "prerequisite_task_id": "task-1",
            ]],
            "created_at_unix_ms": 1,
        ]
    }

    private func messageGraph() -> [String: Any] {
        let tasks = [
            task("task-1", version: 1, clientRef: "research"),
            task("task-2", version: 2, clientRef: "implement"),
            task(
                "task-3",
                version: 1,
                clientRef: "review",
                contextRefs: ["research"]
            ),
        ]
        return [
            "root_task_ids": ["task-1", "task-2", "task-3"],
            "nodes": tasks.map { task in
                [
                    "task": task,
                    "depth": 0,
                    "is_root": true,
                    "is_current_message": true,
                ]
            },
            "edges": [
                [
                    "source_task_id": "task-1",
                    "target_task_id": "task-2",
                    "kind": "prerequisite",
                ],
                [
                    "source_task_id": "task-1",
                    "target_task_id": "task-3",
                    "kind": "context",
                ],
            ],
            "source_conversation_id": "conversation-1",
            "source_turn_id": "turn-1",
            "source_user_message_id": "message-1",
        ]
    }

    private func task(
        _ id: String,
        version: Int,
        clientRef: String,
        contextRefs: [String] = []
    ) -> [String: Any] {
        let runtimeSettings: Any = id == "task-2"
            ? [
                "selected_thinking_level": "high",
                "reasoning_enabled": true,
            ]
            : NSNull()
        return [
            "graph_id": "graph-1",
            "owner_user_id": "user-1",
            "source_entity_type": "conversation_turn",
            "source_entity_id": "turn-1",
            "task_id": id,
            "title": id,
            "model_config_ref": "model-1",
            "model_config_revision": "revision-1",
            "input": [
                "objective": "Ship locally",
                "runtime_settings": runtimeSettings,
                "tool_options": [
                    "plugin_hints": [[
                        "plugin_key": "document",
                        "component_key": "document-mcp",
                    ]],
                    "release_snapshots": [[
                        "plugin_id": "plugin-document",
                        "plugin_key": "document",
                        "component_key": "document-mcp",
                    ]],
                ],
                "input_payload": [
                    "execution_client_ref": clientRef,
                    "dependency_context_refs": contextRefs,
                ],
            ],
            "status": id == "task-1" ? "succeeded" : "running",
            "active_run_id": id == "task-2" ? "run-task-2" : NSNull(),
            "version": version,
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 2,
        ]
    }

    private func run() -> [String: Any] {
        [
            "run_id": "run-task-2",
            "owner_user_id": "user-1",
            "owner_entity_type": "task",
            "owner_entity_id": "task-2",
            "profile_key": "task_execution",
            "capability_policy_revision": "policy-1",
            "input": [:],
            "status": runError == nil ? "succeeded" : "needs_review",
            "version": 3,
            "terminal_outcome": runError == nil ? [
                "content": "Completed locally",
                "reasoning": "private model output",
            ] : NSNull(),
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 2,
        ]
    }

    private func event(cursor: Int, type: String, payload: [String: Any]) -> [String: Any] {
        [
            "cursor": cursor,
            "event_id": "event-\(cursor)",
            "run_id": "run-task-2",
            "event_type": type,
            "payload": payload,
            "created_at_unix_ms": cursor,
        ]
    }

    private func json(_ value: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: value)
    }
}
