import ChatOSAgentRuntime
import SwiftUI

struct AgentRuntimeSettingsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var saved = AgentRuntimePreferences()
    @State private var values: [Field: String] = [:]
    @State private var error = false
    @State private var didSave = false
    @State private var hasSavedPreferences = false
    private let store = AgentSettingsStore()

    private enum Field: String, CaseIterable {
        case calls, approval, story, retries, requestTimeout, runTimeout, noProgress
        case window, reserve
        var title: (String, String) {
            switch self {
            case .calls: ("全局模型调用上限", "Global model call limit")
            case .approval: ("审批调用上限（留空继承）", "Approval call limit (blank inherits)")
            case .story: ("剧情调用上限（留空继承）", "Story call limit (blank inherits)")
            case .retries: ("单次请求重试次数", "Retries per request")
            case .requestTimeout: ("单次请求超时（秒）", "Request timeout (seconds)")
            case .runTimeout: ("整次运行时限（秒）", "Run timeout (seconds)")
            case .noProgress: ("连续无进展暂停阈值", "No-progress round limit")
            case .window: ("模型窗口预算", "Model context window budget")
            case .reserve: ("输出预留 tokens", "Reserved output tokens")
            }
        }
        var range: String {
            switch self {
            case .calls, .approval, .story: "1–10000"
            case .retries: "0–10"
            case .requestTimeout: "5–1800"
            case .runTimeout: "10–86400"
            case .noProgress: "1–100"
            case .window: "2048–2000000"
            case .reserve: "≥ 256"
            }
        }
    }

    var body: some View {
        SettingsGroupedPage {
            LocalConnectorCard(model.localized("Agent 运行", english: "Agent Runtime"),
                subtitle: model.localized("保存于这台 Mac，修改对下一次运行生效。默认最多调用模型 600 次；单次请求默认重试 5 次，并采用 1、2、4、8、16 秒指数退避。重试也计入调用次数。", english: "Stored on this Mac; changes apply to the next run. The defaults are 600 model calls and five retries per request with 1, 2, 4, 8, and 16-second exponential backoff. Retries count as model calls."),
                systemImage: "arrow.triangle.2.circlepath") {
                VStack(spacing: 12) {
                    ForEach([Field.calls, .approval, .story, .retries, .requestTimeout, .runTimeout, .noProgress], id: \.self) { field in row(field) }
                    Divider()
                    if hasSavedPreferences {
                        Text(model.localized("已保存生效值：审批 \(saved.effective(.approval).maximumModelCalls) 次 · 剧情 \(saved.effective(.story).maximumModelCalls) 次", english: "Saved limits: approval \(saved.effective(.approval).maximumModelCalls) calls · story \(saved.effective(.story).maximumModelCalls) calls"))
                            .appFont(.caption).foregroundStyle(.secondary).frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
            LocalConnectorCard(model.localized("上下文窗口", english: "Context Window"),
                subtitle: model.localized("本轮开始或恢复时从 Memory Engine 读取已总结内容和未总结记录。所有文本模型统一通过 OpenAI Responses 协议调用，并在轮内使用 200k server-side compaction。", english: "At the start or resume boundary, summarized context and unsummarized records are read from Memory Engine. Every text model uses the OpenAI Responses protocol with 200k server-side compaction during the run."),
                systemImage: "text.alignleft") {
                VStack(spacing: 12) {
                    ForEach([Field.window, .reserve], id: \.self) { field in row(field) }
                    Divider()
                    if hasSavedPreferences {
                        let context = saved.global.context ?? AgentContextPolicy()
                        Text(model.localized("当前已保存生效值：窗口 \(context.windowTokens) · 预留 \(context.outputReserveTokens)", english: "Current saved values: window \(context.windowTokens) · reserve \(context.outputReserveTokens)"))
                            .appFont(.caption.monospacedDigit()).foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    Text(model.localized("审批和剧情规划使用同一套本机循环。审批不会自动上传记录；剧情规划持续把记录同步到 Memory Engine，但只在每次开始或恢复时 compose 一次。Memory Engine 的摘要由服务端后台自动调度。", english: "Approval and story planning use the same native loop. Approval records are not uploaded automatically. Story planning keeps syncing records to Memory Engine, but composes once per start or resume. Memory Engine summaries are scheduled automatically in the background."))
                        .appFont(.caption).foregroundStyle(.secondary)
                    Text(model.localized("模型请求会发送 context_management.compaction，并把完整 response.output 追加到下一次 input，使用无状态 Responses 链继续运行；Memory Engine 仍完整记录用户、助手、工具调用和工具结果，但不再处理轮内溢出。", english: "Model requests send context_management.compaction and append the complete response.output to the next input for stateless Responses chaining. Memory Engine still records every user message, assistant message, tool call, and tool result, but no longer handles in-run overflow."))
                        .appFont(.caption).foregroundStyle(.secondary)
                }
            }
            HStack {
                Button(model.localized("恢复默认草稿", english: "Reset Draft to Defaults")) { populate(.init()); error = false; didSave = false }
                Spacer()
                Button(model.localized("保存", english: "Save"), action: save).buttonStyle(.borderedProminent)
            }
            if error {
                Text(model.localized("设置未保存：请检查整数、标注范围和窗口预算关系。若原配置损坏，可恢复默认草稿后保存。", english: "Settings were not saved. Check integers, ranges and context budget constraints. If stored settings are invalid, reset the draft and save."))
                    .foregroundStyle(.orange)
            } else if didSave {
                Text(model.localized("已保存，下次运行生效。", english: "Saved. Applies to the next run.")).foregroundStyle(.secondary)
            }
        }
        .task {
            do { saved = try store.load(); populate(saved); hasSavedPreferences = true }
            catch { populate(.init()); self.error = true }
        }
    }

    private func row(_ field: Field) -> some View {
        HStack(spacing: 16) {
            Text(model.localized(field.title.0, english: field.title.1)).frame(maxWidth: .infinity, alignment: .leading)
            Text(field.range).appFont(.caption).foregroundStyle(.secondary)
            TextField("", text: Binding(get: { values[field] ?? "" }, set: { values[field] = $0; didSave = false }))
                .textFieldStyle(.roundedBorder).frame(width: 120)
                .accessibilityLabel(model.localized(field.title.0, english: field.title.1))
        }
    }
    private func populate(_ preferences: AgentRuntimePreferences) {
        let policy = preferences.global
        let context = policy.context ?? .init()
        values = [.calls: "\(policy.maximumModelCalls)", .approval: preferences.approvalMaximumCalls.map(String.init) ?? "",
                  .story: preferences.storyMaximumCalls.map(String.init) ?? "", .retries: "\(policy.maximumRequestRetries)",
                  .requestTimeout: "\(policy.requestTimeoutSeconds)", .runTimeout: "\(policy.runTimeoutSeconds)",
                  .noProgress: "\(policy.maximumNoProgressRounds)", .window: "\(context.windowTokens)", .reserve: "\(context.outputReserveTokens)"]
    }
    private func number(_ field: Field) throws -> Int {
        guard let value = Int((values[field] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)) else { throw AgentRuntimeError.invalidPolicy }
        return value
    }
    private func optionalNumber(_ field: Field) throws -> Int? {
        (values[field] ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : try number(field)
    }
    private func save() {
        do {
            var preferences = AgentRuntimePreferences()
            preferences.global.maximumModelCalls = try number(.calls)
            preferences.approvalMaximumCalls = try optionalNumber(.approval)
            preferences.storyMaximumCalls = try optionalNumber(.story)
            preferences.global.maximumRequestRetries = try number(.retries)
            preferences.global.requestTimeoutSeconds = try number(.requestTimeout)
            preferences.global.runTimeoutSeconds = try number(.runTimeout)
            preferences.global.maximumNoProgressRounds = try number(.noProgress)
            var context = AgentContextPolicy()
            context.windowTokens = try number(.window); context.outputReserveTokens = try number(.reserve)
            preferences.global.context = context
            try store.save(preferences)
            saved = preferences; didSave = true; error = false
            hasSavedPreferences = true
        } catch { self.error = true; didSave = false }
    }
}
