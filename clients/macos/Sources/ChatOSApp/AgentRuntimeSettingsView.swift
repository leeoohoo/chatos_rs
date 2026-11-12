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
        var title: (String, String) {
            switch self {
            case .calls: ("全局模型调用上限", "Global model call limit")
            case .approval: ("审批调用上限（留空继承）", "Approval call limit (blank inherits)")
            case .story: ("剧情调用上限（留空继承）", "Story call limit (blank inherits)")
            case .retries: ("单次请求重试次数", "Retries per request")
            case .requestTimeout: ("流式静默超时（秒）", "Stream inactivity timeout (seconds)")
            case .runTimeout: ("整次运行时限（秒）", "Run timeout (seconds)")
            case .noProgress: ("连续无进展暂停阈值", "No-progress round limit")
            }
        }
        var range: String {
            switch self {
            case .calls, .approval, .story: "1–10000"
            case .retries: "0–10"
            case .requestTimeout: "5–1800"
            case .runTimeout: "10–86400"
            case .noProgress: "1–100"
            }
        }
    }

    var body: some View {
        SettingsGroupedPage {
            LocalConnectorCard(model.localized("Agent 运行", english: "Agent Runtime"),
                subtitle: model.localized("保存于这台 Mac，修改对下一次运行生效。流式静默超时只限制首包或相邻数据块的等待时间，整次运行时限仍是绝对上限。默认最多调用模型 600 次；单次请求默认重试 2 次，并采用 1、2 秒退避。重试也计入调用次数。", english: "Stored on this Mac; changes apply to the next run. The stream inactivity timeout limits only the wait for the first or next body chunk; the run timeout remains the absolute deadline. The defaults are 600 model calls and two retries per request with 1- and 2-second backoff. Retries count as model calls."),
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
            LocalConnectorCard(model.localized("模型输入能力", english: "Model Input Capacity"),
                subtitle: model.localized("上下文窗口和输出预留由所选模型已发布的能力配置决定，运行开始时冻结。缺少配置时会明确报错。", english: "The selected model's published capability determines its context window and output reserve, frozen at the start of each run. Missing configuration is reported explicitly."),
                systemImage: "text.alignleft") {
                Text(model.localized("开始或恢复运行时从 Memory Engine 读取上下文；轮内压缩阈值按当前模型能力计算。", english: "Context is read from Memory Engine at the start or resume boundary; in-run compaction uses the current model's capability."))
                    .appFont(.caption).foregroundStyle(.secondary)
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
        values = [.calls: "\(policy.maximumModelCalls)", .approval: preferences.approvalMaximumCalls.map(String.init) ?? "",
                  .story: preferences.storyMaximumCalls.map(String.init) ?? "", .retries: "\(policy.maximumRequestRetries)",
                  .requestTimeout: "\(policy.requestTimeoutSeconds)", .runTimeout: "\(policy.runTimeoutSeconds)",
                  .noProgress: "\(policy.maximumNoProgressRounds)"]
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
            try store.save(preferences)
            saved = preferences; didSave = true; error = false
            hasSavedPreferences = true
        } catch { self.error = true; didSave = false }
    }
}
