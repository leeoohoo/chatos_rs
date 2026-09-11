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
        case window, reserve, threshold, compactions, summaryTimeout, summaryPoll
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
            case .threshold: ("输入压缩触发阈值", "Input compaction threshold")
            case .compactions: ("单次输入最多压缩次数", "Compaction passes per input")
            case .summaryTimeout: ("等待摘要超时（秒）", "Summary wait timeout (seconds)")
            case .summaryPoll: ("摘要状态查询间隔（秒）", "Summary polling interval (seconds)")
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
            case .threshold: "≥ 512"
            case .compactions: "1–16"
            case .summaryTimeout: "5–1800"
            case .summaryPoll: "1–30"
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
            LocalConnectorCard(model.localized("上下文窗口与压缩", english: "Context Window & Compaction"),
                subtitle: model.localized("默认预算与 Task Runner 一致：250k 窗口、30k 输出预留、220k 触发压缩。输入按完整 JSON 请求约 4 字节/token 估算；支持精确计数的模型接入后可替换估算。", english: "Defaults match Task Runner: a 250k window, 30k output reserve, and compaction at 220k. Input is estimated from the complete JSON request at about four bytes per token; an exact provider count can replace the estimate when supported."),
                systemImage: "text.alignleft") {
                VStack(spacing: 12) {
                    ForEach([Field.window, .reserve, .threshold, .compactions, .summaryTimeout, .summaryPoll], id: \.self) { field in row(field) }
                    Divider()
                    if hasSavedPreferences {
                        let context = saved.global.context ?? AgentContextPolicy()
                        Text(model.localized("当前已保存生效值：窗口 \(context.windowTokens) · 预留 \(context.outputReserveTokens) · 压缩阈值 \(context.compactionThresholdTokens) · 最多 \(context.maximumCompactionPasses) 次", english: "Current saved values: window \(context.windowTokens) · reserve \(context.outputReserveTokens) · threshold \(context.compactionThresholdTokens) · up to \(context.maximumCompactionPasses) passes"))
                            .appFont(.caption.monospacedDigit()).foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    Text(model.localized("审批和剧情规划已使用公共循环。审批不会自动上传记录；剧情规划按 Task Runner 的 Memory Engine 流程同步记录、compose 上下文，并在达到软阈值时触发 active summary。", english: "Approval and story planning use the shared loop. Approval records are not uploaded automatically. Story planning follows Task Runner's Memory Engine flow: sync records, compose context, and trigger active summary at the soft threshold."))
                        .appFont(.caption).foregroundStyle(.secondary)
                    Text(model.localized("摘要使用 Memory Engine 配置的摘要 Agent，可能产生额外模型费用，不计入这里的 600 次调用。等待摘要或视频任务不消耗模型调用次数。", english: "Summaries use the Agent configured in Memory Engine and may incur additional model costs outside this 600-call budget. Waiting for summaries or videos does not consume model calls."))
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
                  .noProgress: "\(policy.maximumNoProgressRounds)", .window: "\(context.windowTokens)", .reserve: "\(context.outputReserveTokens)",
                  .threshold: "\(context.compactionThresholdTokens)", .compactions: "\(context.maximumCompactionPasses)",
                  .summaryTimeout: "\(context.summaryTimeoutSeconds)", .summaryPoll: "\(context.summaryPollSeconds)"]
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
            context.compactionThresholdTokens = try number(.threshold); context.maximumCompactionPasses = try number(.compactions)
            context.summaryTimeoutSeconds = try number(.summaryTimeout); context.summaryPollSeconds = try number(.summaryPoll)
            preferences.global.context = context
            try store.save(preferences)
            saved = preferences; didSave = true; error = false
            hasSavedPreferences = true
        } catch { self.error = true; didSave = false }
    }
}
