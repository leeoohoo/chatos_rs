import Foundation
import XCTest
@testable import ChatOSAgentRuntime

final class AgentRuntimeTests: XCTestCase {
    func testDefaultBudgetIs600() throws {
        let settings = AgentRuntimePreferences()
        XCTAssertEqual(settings.effective(.story).maximumModelCalls, 600)
        XCTAssertEqual(settings.effective(.approval).maximumModelCalls, 600)
        XCTAssertEqual(settings.global.maximumRequestRetries, 2)
        let context = try XCTUnwrap(settings.global.context ?? AgentContextPolicy())
        XCTAssertEqual(context.windowTokens, 200_000)
        XCTAssertEqual(context.outputReserveTokens, 30_000)
        try settings.validate()
    }

    func testManagedSettingsOverrideLegacyLocalPreferences() throws {
        let name = "AgentManagedSettingsTests.\(UUID())"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: name))
        defer { defaults.removePersistentDomain(forName: name) }
        var legacy = AgentRuntimePreferences()
        legacy.global.maximumModelCalls = 111
        defaults.set(try JSONEncoder().encode(legacy), forKey: "chatos.agent-runtime.settings.v1")

        var managed = AgentRuntimePreferences()
        managed.global.maximumModelCalls = 725
        managed.global.context = AgentContextPolicy()
        let store = AgentSettingsStore(suiteName: name)
        try store.saveManaged(managed)

        XCTAssertEqual(try store.load(), managed)
    }

    func testManagedSettingsSkipIdenticalWritesAndStillRemoveLegacyValue() throws {
        let name = "AgentManagedSettingsIdempotencyTests.\(UUID())"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: name))
        defer { defaults.removePersistentDomain(forName: name) }
        var managed = AgentRuntimePreferences()
        managed.global.maximumModelCalls = 725
        let store = AgentSettingsStore(suiteName: name)

        XCTAssertTrue(try store.saveManagedIfChanged(managed))
        for _ in 0..<20 {
            XCTAssertFalse(try store.saveManagedIfChanged(managed))
        }

        defaults.set(Data("legacy".utf8), forKey: "chatos.agent-runtime.settings.v1")
        XCTAssertTrue(try store.saveManagedIfChanged(managed))
        XCTAssertNil(defaults.object(forKey: "chatos.agent-runtime.settings.v1"))
        for _ in 0..<20 {
            XCTAssertFalse(try store.saveManagedIfChanged(managed))
        }
    }

    func testLegacyRetryDefaultMigratesOnceWithoutResettingContextSettings() throws {
        let name = "AgentRetryMigrationTests.\(UUID())"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: name))
        defer { defaults.removePersistentDomain(forName: name) }
        var old = AgentRuntimePreferences()
        old.global.maximumRequestRetries = 5
        var context = AgentContextPolicy()
        context.windowTokens = 2_000_000
        context.outputReserveTokens = 30_000
        old.global.context = context
        defaults.set(try JSONEncoder().encode(old), forKey: "chatos.agent-runtime.settings.v1")

        let migrated = try AgentSettingsStore(suiteName: name).load()
        XCTAssertEqual(migrated.global.maximumRequestRetries, 2)
        XCTAssertEqual(migrated.global.context?.windowTokens, 2_000_000)

        var explicitlyChanged = migrated
        explicitlyChanged.global.maximumRequestRetries = 5
        try AgentSettingsStore(suiteName: name).save(explicitlyChanged)
        XCTAssertEqual(try AgentSettingsStore(suiteName: name).load().global.maximumRequestRetries, 5)
    }

    func testContextEstimateReturnsApproximateTokensRatherThanRawBytes() throws {
        let messages = [AgentMessage(role: .user, content: String(repeating: "x", count: 4_000))]
        let estimate = try AgentContextBudget.estimate(messages: messages, tools: [])
        XCTAssertGreaterThan(estimate, 900)
        XCTAssertLessThan(estimate, 1_200)
    }

    func testLegacyCheckpointWithoutInstructionBundlesStillDecodes() throws {
        var checkpoint = AgentRunCheckpoint(
            scope: "legacy",
            messages: [.init(role: .system, content: "persisted instructions")]
        )
        checkpoint.instructionBundles = [
            .init(
                name: "future-skill",
                version: 2,
                contentSHA256: String(repeating: "a", count: 64),
                language: "en-US",
                audience: "manager"
            ),
        ]
        let encoded = try JSONEncoder().encode(checkpoint)
        var object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: encoded) as? [String: Any]
        )
        object.removeValue(forKey: "instructionBundles")

        let legacyData = try JSONSerialization.data(withJSONObject: object)
        let decoded = try JSONDecoder().decode(AgentRunCheckpoint.self, from: legacyData)

        XCTAssertEqual(decoded.scope, "legacy")
        XCTAssertEqual(decoded.messages.first?.content, "persisted instructions")
        XCTAssertTrue(decoded.instructionBundleItems.isEmpty)
    }

    func testDeterministicCompletionCheckFinishesWithoutAnotherModelCall() async throws {
        let checkpoint = AgentRunCheckpoint(scope: "test", messages: [.init(role: .user, content: "work")])
        let model = CompletionCheckModel()
        let result = try await AgentRuntime().run(
            checkpoint: checkpoint, scope: "test", policy: .init(), model: model, tools: [],
            execute: { _ in .failure("unexpected") }, completionCheck: { "validated result" }
        )
        XCTAssertEqual(result.status, .completed)
        XCTAssertEqual(result.result, "validated result")
        XCTAssertEqual(result.modelCalls, 0)
        let callCount = await model.callCount
        XCTAssertEqual(callCount, 0)
    }

    func testToollessSummaryReceivesSpecificIncompletionReminder() async throws {
        let checkpoint = AgentRunCheckpoint(
            scope: "specific-reminder",
            messages: [.init(role: .user, content: "完成任务")]
        )
        let model = SpecificReminderModel()
        let result = try await AgentRuntime().run(
            checkpoint: checkpoint,
            scope: "specific-reminder",
            policy: .init(),
            model: model,
            tools: [.init(
                name: "finish",
                description: "结束",
                schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8),
                effect: .terminal
            )],
            execute: { call in
                XCTAssertEqual(call.name, "finish")
                return .init("done")
            },
            incompletionReminder: {
                "仍未通过：#2 [failed] 编译并运行测试。请继续调用工具。"
            }
        )
        XCTAssertEqual(result.status, .completed)
        XCTAssertEqual(result.result, "done")
        let sawSpecificReminder = await model.sawSpecificReminder
        XCTAssertTrue(sawSpecificReminder)
    }
}

private actor CompletionCheckModel: AgentModelClient {
    var callCount = 0
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) async throws -> AgentMessage {
        callCount += 1
        return .init(role: .assistant)
    }
}

private actor SpecificReminderModel: AgentModelClient {
    private(set) var sawSpecificReminder = false
    private var callCount = 0

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        callCount += 1
        if callCount == 1 {
            return .init(role: .assistant, content: "我已完成。")
        }
        sawSpecificReminder = messages.contains {
            $0.role == .user && $0.content.contains("#2 [failed] 编译并运行测试")
        }
        return .init(role: .assistant, toolCalls: [.init(
            id: "finish-after-reminder",
            name: "finish",
            arguments: "{}"
        )])
    }
}
