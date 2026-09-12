// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation
import Testing

@Suite("Shared Local Agent protocol v12 fixtures")
struct LocalAgentProtocolV12FixtureTests {
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

        #expect(localAgentProtocolVersion == 12)
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
        #expect(detail.run.run.runID == "task-run-2")
        #expect(detail.run.resultSummary == "Design implemented")
        #expect(detail.eventsTotal == 1)
    }

    private func fixtureURL(_ name: String) -> URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("shared/fixtures/local_agent/v12")
            .appendingPathComponent(name)
    }
}
