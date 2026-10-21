import ChatOSCore
import Foundation

enum NativeLocalAgentRunActivityProjector {
    static func project(
        run: LocalAgentRunRecord,
        pendingApprovals: [LocalAgentToolInvocationRecord] = []
    ) -> MessageTaskRunActivity? {
        let startedAt = Date(timeIntervalSince1970: TimeInterval(run.updatedAtUnixMs) / 1_000)
        let pendingCall = firstPendingCall(in: run.pendingToolBatch)
        let approval = pendingApprovals.first { $0.runID == run.runID }

        switch run.status {
        case "queued", "model_ready":
            return .init(
                kind: .waitingModel,
                detail: recentResultDetail(in: run.continuationInput),
                startedAt: startedAt
            )
        case "model_running":
            return .init(
                kind: .runningModel,
                detail: recentResultDetail(in: run.continuationInput),
                startedAt: startedAt
            )
        case "waiting_user":
            return .init(
                kind: .waitingUser,
                operation: pendingCall?.toolName,
                detail: promptDetail(in: run.continuationInput) ?? promptDetail(in: run.checkpoint),
                startedAt: startedAt
            )
        case "needs_review":
            return .init(
                kind: .waitingApproval,
                operation: approval?.toolName ?? pendingCall?.toolName,
                detail: approval?.approvalReason ?? approval.flatMap(purpose),
                startedAt: startedAt
            )
        case "waiting_tool_result":
            if let approval {
                return .init(
                    kind: .waitingApproval,
                    operation: approval.toolName,
                    detail: approval.approvalReason ?? purpose(approval),
                    startedAt: startedAt
                )
            }
            guard let call = pendingCall else {
                return .init(kind: .waitingTool, startedAt: startedAt)
            }
            return activity(for: call, startedAt: startedAt)
        case "continuation_ready":
            if let recent = recentInvocation(in: run.continuationInput),
               ["tool_result_read", "tool_result_query"].contains(recent.toolName) {
                return .init(
                    kind: .readingLargeResult,
                    operation: recent.toolName,
                    detail: resultQueryDetail(arguments: recent.arguments),
                    startedAt: startedAt
                )
            }
            return .init(kind: .waitingModel, startedAt: startedAt)
        case "retry_scheduled":
            let detail = run.nextAttemptAtUnixMs.map {
                Date(timeIntervalSince1970: TimeInterval($0) / 1_000).formatted()
            }
            return .init(kind: .retryScheduled, detail: detail, startedAt: startedAt)
        case "paused":
            return .init(kind: .paused, startedAt: startedAt)
        default:
            return nil
        }
    }

    private static func activity(
        for call: PendingCall,
        startedAt: Date
    ) -> MessageTaskRunActivity {
        switch call.toolName {
        case "ask_user_prompt_key_values", "ask_user_prompt_choices", "ask_user_prompt_mixed_form":
            return .init(
                kind: .waitingUser,
                operation: call.toolName,
                detail: string("title", in: call.arguments) ?? string("prompt", in: call.arguments),
                startedAt: startedAt
            )
        case "tool_result_read", "tool_result_query":
            return .init(
                kind: .readingLargeResult,
                operation: call.toolName,
                detail: resultQueryDetail(arguments: call.arguments),
                startedAt: startedAt
            )
        case "capability_describe":
            let selection = [
                string("plugin_option", in: call.arguments),
                string("tool_option", in: call.arguments),
            ].compactMap { $0 }.joined(separator: " / ")
            return .init(
                kind: .restoringCapability,
                operation: call.toolName,
                detail: selection.isEmpty ? nil : selection,
                startedAt: startedAt
            )
        case "task_run_process_allocate_service":
            let name = string("service_name", in: call.arguments)
            let preferred = integer("preferred_port", in: call.arguments).map { ":\($0)" }
            return .init(
                kind: .startingProcess,
                operation: call.toolName,
                detail: [name, preferred].compactMap { $0 }.joined(),
                startedAt: startedAt
            )
        case "execute_command" where bool("background", in: call.arguments) == true:
            return .init(
                kind: .startingProcess,
                operation: call.toolName,
                detail: string("purpose", in: call.arguments),
                startedAt: startedAt
            )
        default:
            return .init(
                kind: .waitingTool,
                operation: call.toolName,
                detail: string("purpose", in: call.arguments),
                startedAt: startedAt
            )
        }
    }

    private static func resultQueryDetail(arguments: LocalAgentJSONValue) -> String? {
        let parts = [
            string("mode", in: arguments),
            string("json_pointer", in: arguments),
            string("search", in: arguments),
            integer("offset", in: arguments).map { "offset \($0)" },
        ].compactMap { $0 }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    private static func recentResultDetail(in value: LocalAgentJSONValue?) -> String? {
        guard let invocation = recentInvocation(in: value) else { return nil }
        if invocation.toolName == "task_run_process_allocate_service",
           let url = recursiveString("service_url", in: invocation.result) {
            return url
        }
        if ["tool_result_read", "tool_result_query"].contains(invocation.toolName) {
            return resultQueryDetail(arguments: invocation.arguments)
        }
        return nil
    }

    private static func promptDetail(in value: LocalAgentJSONValue?) -> String? {
        recursiveString("title", in: value) ?? recursiveString("prompt", in: value)
    }

    private struct PendingCall {
        let toolName: String
        let arguments: LocalAgentJSONValue
    }

    private struct RecentInvocation {
        let toolName: String
        let arguments: LocalAgentJSONValue
        let result: LocalAgentJSONValue?
    }

    private static func firstPendingCall(in value: LocalAgentJSONValue?) -> PendingCall? {
        guard case let .object(batch)? = value,
              case let .array(calls)? = batch["calls"] ?? batch["tool_calls"] else { return nil }
        for call in calls {
            guard case let .object(object) = call,
                  case let .string(name)? = object["tool_name"] else { continue }
            return PendingCall(toolName: name, arguments: object["arguments"] ?? .object([:]))
        }
        return nil
    }

    private static func recentInvocation(in value: LocalAgentJSONValue?) -> RecentInvocation? {
        guard case let .object(container)? = value,
              case let .array(invocations)? = container["invocations"] else { return nil }
        for invocation in invocations.reversed() {
            guard case let .object(object) = invocation,
                  case let .string(name)? = object["tool_name"] else { continue }
            return RecentInvocation(
                toolName: name,
                arguments: object["arguments"] ?? .object([:]),
                result: object["result"]
            )
        }
        return nil
    }

    private static func purpose(_ invocation: LocalAgentToolInvocationRecord) -> String? {
        string("purpose", in: invocation.arguments)
    }

    private static func string(_ key: String, in value: LocalAgentJSONValue) -> String? {
        guard case let .object(object) = value,
              case let .string(result)? = object[key] else { return nil }
        let trimmed = result.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    private static func integer(_ key: String, in value: LocalAgentJSONValue) -> Int? {
        guard case let .object(object) = value,
              case let .number(result)? = object[key] else { return nil }
        return Int(exactly: result)
    }

    private static func bool(_ key: String, in value: LocalAgentJSONValue) -> Bool? {
        guard case let .object(object) = value,
              case let .bool(result)? = object[key] else { return nil }
        return result
    }

    private static func recursiveString(
        _ key: String,
        in value: LocalAgentJSONValue?
    ) -> String? {
        guard let value else { return nil }
        switch value {
        case let .object(object):
            if case let .string(result)? = object[key] { return result }
            return object.values.lazy.compactMap { recursiveString(key, in: $0) }.first
        case let .array(values):
            return values.lazy.compactMap { recursiveString(key, in: $0) }.first
        default:
            return nil
        }
    }
}
