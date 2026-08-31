import ChatOSCore
import SwiftUI

struct LocalConnectorModelDefaultsSection: View {
    @EnvironmentObject private var appModel: AppModel
    var models: [LocalConnectorModelConfig]
    @Binding var settings: LocalConnectorModelSettings

    var body: some View {
        LocalConnectorCard(
            appModel.localized("默认模型", english: "Default Models"),
            subtitle: appModel.localized(
                "默认模型只从已启用且凭据可用的配置中选择。",
                english: "Default models can only use enabled configurations with valid credentials."
            ),
            systemImage: "switch.2"
        ) {
            VStack(spacing: 12) {
                defaultModelRow(
                    title: appModel.localized("Memory 总结", english: "Memory Summary"),
                    subtitle: appModel.localized(
                        "用于压缩长期会话记忆与生成摘要。",
                        english: "Compresses long-term conversation memory and generates summaries."
                    ),
                    modelID: binding(\.memorySummaryModelConfigID),
                    thinking: binding(\.memorySummaryThinkingLevel)
                )
                Divider()
                defaultModelRow(
                    title: appModel.localized("项目管理 Agent", english: "Project Management Agent"),
                    subtitle: appModel.localized(
                        "用于拆解需求、规划任务节点和推进项目。",
                        english: "Breaks down requirements, plans task nodes, and advances projects."
                    ),
                    modelID: binding(\.projectManagementAgentModelConfigID),
                    thinking: binding(\.projectManagementAgentThinkingLevel)
                )
                Divider()
                defaultModelRow(
                    title: appModel.localized("本机自动审批 Agent", english: "Local Approval Agent"),
                    subtitle: appModel.localized(
                        "在本机只读检查项目上下文后，决定批准、拒绝或转交用户。",
                        english: "Reviews project context locally in read-only mode, then approves, rejects, or asks the user."
                    ),
                    modelID: binding(\.commandApprovalModelConfigID),
                    thinking: binding(\.commandApprovalThinkingLevel)
                )
            }
        }
    }

    private var runnableModels: [LocalConnectorModelConfig] {
        models.filter { $0.enabled && $0.taskEnabled && $0.hasAPIKey }
    }

    private func defaultModelRow(
        title: String,
        subtitle: String,
        modelID: Binding<String?>,
        thinking: Binding<String?>
    ) -> some View {
        let selected = runnableModels.first(where: { $0.id == modelID.wrappedValue })
        let options = LocalConnectorThinkingOptions.options(provider: selected?.provider)
        return VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(title).appFont(.headline)
                    Text(subtitle).appFont(.caption).foregroundStyle(.secondary)
                }
                Spacer()
            }
            HStack(spacing: 12) {
                Picker(appModel.localized("模型", english: "Model"), selection: Binding(
                    get: { modelID.wrappedValue ?? "" },
                    set: { next in
                        modelID.wrappedValue = next.isEmpty ? nil : next
                        let provider = runnableModels.first(where: { $0.id == next })?.provider
                        thinking.wrappedValue = LocalConnectorThinkingOptions
                            .normalizedValue(thinking.wrappedValue, provider: provider)
                            .nilIfEmpty
                    }
                )) {
                    Text(appModel.localized("未配置", english: "Not configured")).tag("")
                    ForEach(runnableModels) { model in
                        Text("\(model.name) · \(model.modelName)").tag(model.id)
                    }
                }
                .labelsHidden()
                .frame(maxWidth: .infinity)

                Picker("Thinking", selection: Binding(
                    get: { thinking.wrappedValue ?? "" },
                    set: { thinking.wrappedValue = $0.isEmpty ? nil : $0 }
                )) {
                    ForEach(options) { option in
                        Text(option.label).tag(option.value)
                    }
                }
                .frame(width: 170)
                .disabled(selected == nil)
            }
        }
        .padding(.vertical, 2)
    }

    private func binding<T>(_ keyPath: WritableKeyPath<LocalConnectorModelSettings, T>) -> Binding<T> {
        Binding(
            get: { settings[keyPath: keyPath] },
            set: { settings[keyPath: keyPath] = $0 }
        )
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
