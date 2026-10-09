import ChatOSCore
import SwiftUI

enum AgentModelCatalogStatus: Equatable {
    case notLoaded, loading, ready, failed
}

enum AgentModelAvailability: Equatable {
    case unchecked, loading, available(LocalAgentBuilderModelOption), unavailable, failed

    static func resolve(
        id: String, models: [LocalAgentBuilderModelOption], catalogStatus: AgentModelCatalogStatus
    ) -> Self {
        switch catalogStatus {
        case .notLoaded: .unchecked
        case .loading: .loading
        case .failed: .failed
        case .ready:
            models.first(where: { $0.id == id }).map(Self.available) ?? .unavailable
        }
    }
}

struct AgentModelStatusLabel: View {
    let availability: AgentModelAvailability

    var body: some View {
        switch availability {
        case .unchecked:
            Text("模型状态未确认").foregroundStyle(.secondary)
        case .loading:
            Text("正在检查模型").foregroundStyle(.secondary)
        case let .available(model):
            Text("\(model.name) · \(model.modelName)").lineLimit(1)
        case .unavailable:
            Text("模型不可用")
                .foregroundStyle(.orange)
                .help("当前保存的模型不在可用列表中，请编辑后重新选择。")
        case .failed:
            Text("模型列表加载失败").foregroundStyle(.orange)
        }
    }
}

/// Always provide a tag for the saved selection, even if its model was removed or disabled.
/// Do not silently replace a missing model with the first available model.
struct AgentModelPicker: View {
    @Binding var selection: String
    let models: [LocalAgentBuilderModelOption]

    static func contains(_ id: String, in models: [LocalAgentBuilderModelOption]) -> Bool {
        models.contains(where: { $0.id == id })
    }

    private var hasSelection: Bool { Self.contains(selection, in: models) }

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Picker("模型", selection: $selection) {
                if !hasSelection {
                    if selection.isEmpty {
                        Text("请选择模型").tag(selection)
                    } else {
                        Text("原模型不可用").tag(selection)
                    }
                }
                ForEach(models) { model in
                    Text("\(model.name) · \(model.modelName)").tag(model.id)
                }
            }
            .accessibilityIdentifier("agent-model-picker")
            if !hasSelection && !selection.isEmpty {
                Text("原模型当前不可用，请选择新的模型后保存。")
                    .font(.caption)
                    .foregroundStyle(.orange)
                Text("原模型配置 ID：\(selection)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
        }
    }
}
