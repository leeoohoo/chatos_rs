import ChatOSAgentRuntime
import Foundation
import XCTest
@testable import ChatOSConnector

final class NativeApprovalAgentTests: XCTestCase {
    func testSharedLoopCanInspectBeyondEightRoundsBeforeDeciding() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("approval-agent-tests-\(UUID())")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data((1...12).map { "line \($0)" }.joined(separator: "\n").utf8).write(to: root.appendingPathComponent("sample.txt"))
        let model = ApprovalTestModel(finishAt: 11)
        let result = await NativeApprovalAgent().evaluate(request: request(root), modelClient: model, policy: .init())
        XCTAssertEqual(result, .approve(reason: "checked", rememberAllow: false))
        let calls = await model.calls
        XCTAssertEqual(calls, 11)
        let toolNames = await model.toolNames
        XCTAssertEqual(Set(toolNames), Set(["read_file_raw", "read_file_range", "list_dir", "search_text", "approval_decision"]))
    }

    func testMemoryProviderReceivesApprovalMessagesToolCallsAndResults() async throws {
        let runID = UUID()
        let runtimeScope = "approval:\(runID.uuidString)"
        let scope = try AgentMemoryScope(
            tenantID: "user-1", profile: "approval", projectID: "workspace-1",
            runID: runID, runtimeScope: runtimeScope
        )
        let memory = ApprovalTestMemory()
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)

        let result = await NativeApprovalAgent().evaluate(
            request: request(FileManager.default.temporaryDirectory),
            modelClient: ApprovalTestModel(finishAt: 2), policy: .init(),
            runID: runID, runtimeScope: runtimeScope, contextProvider: provider
        )

        XCTAssertEqual(result, .approve(reason: "checked", rememberAllow: false))
        let messages = await memory.entries.map(\.message)
        XCTAssertEqual(messages.map(\.role), [.system, .user, .assistant, .tool, .assistant, .tool])
        XCTAssertEqual(messages[2].toolCalls.first?.id, "read-1")
        XCTAssertEqual(messages[3].toolCallID, "read-1")
        XCTAssertEqual(messages[4].toolCalls.first?.id, "decision")
        XCTAssertEqual(messages[5].toolCallID, "decision")
        let composeCalls = await memory.composeCalls
        XCTAssertEqual(composeCalls, 1)
    }

    func testExhaustedBudgetAndUnavailableModelAskHuman() async throws {
        var policy = AgentRunPolicy(); policy.maximumModelCalls = 1
        for fail in [false, true] {
            let model = ApprovalTestModel(finishAt: 2, fail: fail)
            let result = await NativeApprovalAgent().evaluate(request: request(FileManager.default.temporaryDirectory), modelClient: model, policy: policy)
            guard case .askUser = result else { return XCTFail("Errors and limits must never grant approval") }
        }
    }

    func testInvalidTerminalDecisionDoesNotApprove() async throws {
        var policy = AgentRunPolicy(); policy.maximumModelCalls = 1
        let result = await NativeApprovalAgent().evaluate(request: request(FileManager.default.temporaryDirectory),
            modelClient: ApprovalTestModel(finishAt: 1, invalidDecision: true), policy: policy)
        guard case .askUser = result else { return XCTFail("Invalid decision cannot approve") }
    }

    func testReadToolStillRejectsTraversalAndAbsolutePaths() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("approval-path-tests-\(UUID())")
        let inside = root.appendingPathComponent("project")
        try FileManager.default.createDirectory(at: inside, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let outside = root.appendingPathComponent("outside.txt")
        try Data("private-outside-content".utf8).write(to: outside)
        for path in ["../outside.txt", outside.path] {
            let result = NativeApprovalAgentTools().execute(name: "read_file_raw", arguments: ["path": path], projectRoot: inside)
            XCTAssertTrue(result.hasPrefix("工具执行失败："))
            XCTAssertFalse(result.contains("private-outside-content"))
        }
    }

    private func request(_ root: URL) -> NativeApprovalAgentRequest {
        .init(command: "read", arguments: ["sample.txt"], cwd: ".", source: "test", projectRoot: root,
              riskLevel: "low", riskReason: nil, requestedPermissionsDescription: nil)
    }
}

private actor ApprovalTestModel: AgentModelClient {
    let finishAt: Int
    let fail: Bool
    let invalidDecision: Bool
    var calls = 0
    var toolNames: [String] = []
    init(finishAt: Int, fail: Bool = false, invalidDecision: Bool = false) {
        self.finishAt = finishAt; self.fail = fail; self.invalidDecision = invalidDecision
    }
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) throws -> AgentMessage {
        calls += 1; toolNames = tools.map(\.name)
        if fail { throw AgentRuntimeError.provider(401) }
        if calls == finishAt {
            return .init(role: .assistant, toolCalls: [.init(id: "decision", name: "approval_decision",
                arguments: invalidDecision ? #"{"decision":"approve","reason":""}"# : #"{"decision":"approve","reason":"checked","remember_allow":false}"#)])
        }
        return .init(role: .assistant, toolCalls: [.init(id: "read-\(calls)", name: "read_file_range",
            arguments: "{\"path\":\"sample.txt\",\"start_line\":\(calls),\"end_line\":\(calls)}")])
    }
}

private actor ApprovalTestMemory: AgentMemoryServicing {
    var entries: [AgentMemoryEntry] = []
    var composeCalls = 0

    func ensureThread() {}

    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) {
        self.entries.append(contentsOf: entries)
    }

    func compose() -> AgentMemoryContext {
        composeCalls += 1
        return .init(
            blocks: [],
            recentRecords: entries.map { .init(id: $0.id, message: $0.message) }
        )
    }
}
