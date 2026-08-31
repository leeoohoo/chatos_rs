import ChatOSCore
import SwiftUI

struct LocalConnectorTaskModelsSection: View {
    @EnvironmentObject private var appModel: AppModel
    var models: [LocalConnectorModelConfig]
    @Binding var drafts: [String: LocalConnectorTaskModelDraft]

    var body: some View {
        LocalConnectorCard(
            appModel.localized("Task Runner 模型", english: "Task Runner Models"),
            subtitle: appModel.localized(
                "每个模型分别配置任务用途、默认 Thinking、Temperature 与最大输出长度。",
                english: "Configure task usage, default thinking, temperature, and maximum output for each model."
            ),
            systemImage: "point.3.connected.trianglepath.dotted"
        ) {
            if models.isEmpty {
                ContentUnavailableView(
                    appModel.localized("还没有模型", english: "No models yet"),
                    systemImage: "brain.head.profile",
                    description: Text(appModel.localized(
                        "请先添加供应商并刷新模型目录。",
                        english: "Add a provider and refresh the model catalog first."
                    ))
                )
                .frame(minHeight: 150)
            } else {
                LazyVStack(spacing: 10) {
                    ForEach(models) { model in
                        LocalConnectorTaskModelRow(
                            model: model,
                            draft: draftBinding(for: model)
                        )
                    }
                }
            }
        }
    }

    private func draftBinding(for model: LocalConnectorModelConfig) -> Binding<LocalConnectorTaskModelDraft> {
        Binding(
            get: { drafts[model.id] ?? .init(model: model) },
            set: { drafts[model.id] = $0 }
        )
    }
}

private struct LocalConnectorTaskModelRow: View {
    @EnvironmentObject private var appModel: AppModel
    var model: LocalConnectorModelConfig
    @Binding var draft: LocalConnectorTaskModelDraft

    var body: some View {
        VStack(alignment: .leading, spacing: 13) {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: model.supportsReasoning ? "sparkles" : "text.bubble")
                    .foregroundStyle(draft.taskEnabled ? Color.accentColor : .secondary)
                    .frame(width: 34, height: 34)
                    .background(.tint.opacity(draft.taskEnabled ? 0.1 : 0.04), in: RoundedRectangle(cornerRadius: 9))
                VStack(alignment: .leading, spacing: 2) {
                    Text(model.name).appFont(.headline)
                    Text("\(model.provider) · \(model.modelName)")
                        .appFont(.caption.monospaced())
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
                Spacer()
                if !model.hasAPIKey {
                    Label(appModel.localized("缺少凭据", english: "Missing credentials"), systemImage: "key.slash")
                        .appFont(.caption)
                        .foregroundStyle(.orange)
                }
                Toggle(
                    appModel.localized("用于任务", english: "Use for Tasks"),
                    isOn: $draft.taskEnabled
                )
                    .toggleStyle(.switch)
                    .controlSize(.small)
            }

            Grid(horizontalSpacing: 12, verticalSpacing: 5) {
                GridRow {
                    fieldTitle(appModel.localized("任务用途", english: "Task Usage"))
                    fieldTitle(appModel.localized("默认 Thinking", english: "Default Thinking"))
                    fieldTitle("Temperature")
                    fieldTitle("Max Tokens")
                }
                GridRow {
                    TextField(
                        appModel.localized(
                            "例如：代码实现、分析、视觉理解",
                            english: "For example: coding, analysis, visual understanding"
                        ),
                        text: $draft.usage
                    )
                        .frame(minWidth: 220)
                    Picker("Thinking", selection: $draft.thinking) {
                        ForEach(LocalConnectorThinkingOptions.options(provider: model.provider)) { option in
                            Text(option.label).tag(option.value)
                        }
                    }
                    .labelsHidden()
                    .frame(width: 150)
                    TextField(appModel.localized("默认", english: "Default"), text: $draft.temperature)
                        .frame(width: 100)
                    TextField(appModel.localized("默认", english: "Default"), text: $draft.maxOutputTokens)
                        .frame(width: 110)
                }
            }
            .textFieldStyle(.roundedBorder)
            .disabled(!draft.taskEnabled)
        }
        .padding(14)
        .background(.background, in: RoundedRectangle(cornerRadius: 12))
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .stroke(.separator.opacity(0.65))
        }
        .opacity(draft.taskEnabled ? 1 : 0.62)
    }

    private func fieldTitle(_ title: String) -> some View {
        Text(title)
            .appFont(.caption)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}
