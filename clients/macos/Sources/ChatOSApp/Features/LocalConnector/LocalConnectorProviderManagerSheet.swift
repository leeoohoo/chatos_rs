import ChatOSCore
import SwiftUI

struct LocalConnectorProviderManagerSheet: View {
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject private var appModel: AppModel
    @ObservedObject var viewModel: LocalConnectorControlCenterViewModel
    @State private var editingProvider: LocalConnectorModelProvider?
    @State private var isCreating = false
    @State private var pendingDeletion: LocalConnectorModelProvider?

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            if isCreating || editingProvider != nil {
                LocalConnectorProviderEditor(
                    provider: editingProvider,
                    disabled: viewModel.isPerformingAction,
                    onCancel: closeEditor,
                    onSave: save
                )
            } else {
                providerList
            }
        }
        .frame(minWidth: 720, minHeight: 560)
        .confirmationDialog(
            appModel.localized("删除供应商？", english: "Delete provider?"),
            isPresented: Binding(
                get: { pendingDeletion != nil },
                set: { if !$0 { pendingDeletion = nil } }
            ),
            presenting: pendingDeletion
        ) { provider in
            Button(appModel.localized(
                "删除 \(provider.name)",
                english: "Delete \(provider.name)"
            ), role: .destructive) {
                viewModel.deleteModelProvider(id: provider.id)
                pendingDeletion = nil
            }
        } message: { provider in
            Text(appModel.localized(
                "这会同时删除由 \(provider.name) 导入的模型配置。",
                english: "This also deletes model configurations imported from \(provider.name)."
            ))
        }
    }

    private var header: some View {
        HStack(spacing: 12) {
            if isCreating || editingProvider != nil {
                Button(appModel.localized("返回", english: "Back"), systemImage: "chevron.left", action: closeEditor)
                    .labelStyle(.iconOnly)
            }
            VStack(alignment: .leading, spacing: 3) {
                Text(isCreating
                     ? appModel.localized("添加供应商", english: "Add Provider")
                     : editingProvider == nil
                        ? appModel.localized("模型供应商", english: "Model Providers")
                        : appModel.localized("修改供应商", english: "Edit Provider"))
                    .appFont(.title2.weight(.semibold))
                Text(isCreating || editingProvider != nil
                     ? appModel.localized(
                        "保存供应商连接信息和模型能力。",
                        english: "Save provider connection details and model capabilities."
                     )
                     : appModel.localized(
                        "供应商负责凭据、Base URL 和模型目录同步。",
                        english: "Providers manage credentials, Base URLs, and model catalog sync."
                     ))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if viewModel.isPerformingAction { ProgressView().controlSize(.small) }
            if !isCreating && editingProvider == nil {
                Button(appModel.localized("添加供应商", english: "Add Provider"), systemImage: "plus") {
                    isCreating = true
                }
                    .buttonStyle(.borderedProminent)
            }
            Button(appModel.localized("关闭", english: "Close"), systemImage: "xmark", action: dismiss.callAsFunction)
                .labelStyle(.iconOnly)
        }
        .padding(20)
    }

    private var providerList: some View {
        Group {
            if viewModel.modelProviders.isEmpty {
                ContentUnavailableView(
                    appModel.localized("还没有模型供应商", english: "No model providers yet"),
                    systemImage: "server.rack",
                    description: Text(appModel.localized(
                        "添加供应商后，刷新目录即可导入可用模型。",
                        english: "Add a provider, then refresh its catalog to import available models."
                    ))
                )
            } else {
                ScrollView {
                    LazyVStack(spacing: 10) {
                        ForEach(viewModel.modelProviders) { provider in
                            providerRow(provider)
                        }
                    }
                    .padding(20)
                }
            }
        }
    }

    private func providerRow(_ provider: LocalConnectorModelProvider) -> some View {
        VStack(alignment: .leading, spacing: 11) {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "server.rack")
                    .foregroundStyle(provider.enabled ? Color.accentColor : .secondary)
                    .frame(width: 38, height: 38)
                    .background(.tint.opacity(0.09), in: RoundedRectangle(cornerRadius: 10))
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 8) {
                        Text(provider.name).appFont(.headline)
                        statusBadge(
                            provider.enabled
                                ? appModel.localized("已启用", english: "Enabled")
                                : appModel.localized("已停用", english: "Disabled"),
                            color: provider.enabled ? .green : .secondary
                        )
                        statusBadge(
                            provider.hasAPIKey
                                ? appModel.localized("凭据已保存", english: "Credentials saved")
                                : appModel.localized("缺少凭据", english: "Missing credentials"),
                            color: provider.hasAPIKey ? .green : .orange
                        )
                    }
                    Text("\(provider.provider) · Prompt: \(provider.promptVendor)")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                    Text(provider.baseURL)
                        .appFont(.caption.monospaced())
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
                Spacer()
                Text(appModel.localized(
                    "\(provider.importedModelCount) 个模型",
                    english: "\(provider.importedModelCount) models"
                ))
                    .appFont(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            if let error = provider.lastSyncError, !error.isEmpty {
                Label(error, systemImage: "exclamationmark.triangle.fill")
                    .appFont(.caption)
                    .foregroundStyle(.orange)
                    .textSelection(.enabled)
            }
            HStack {
                capabilityBadge(appModel.localized("图片", english: "Images"), enabled: provider.supportsImages)
                capabilityBadge(appModel.localized("推理", english: "Reasoning"), enabled: provider.supportsReasoning)
                capabilityBadge("Responses", enabled: provider.supportsResponses)
                Spacer()
                Button(appModel.localized("刷新模型", english: "Refresh Models"), systemImage: "arrow.triangle.2.circlepath") {
                    viewModel.refreshModelProvider(id: provider.id)
                }
                Button(appModel.localized("修改", english: "Edit"), systemImage: "pencil") { editingProvider = provider }
                Button(appModel.localized("删除", english: "Delete"), systemImage: "trash", role: .destructive) {
                    pendingDeletion = provider
                }
            }
            .disabled(viewModel.isPerformingAction)
        }
        .padding(15)
        .background(.background, in: RoundedRectangle(cornerRadius: 13))
        .overlay { RoundedRectangle(cornerRadius: 13).stroke(.separator.opacity(0.65)) }
    }

    private func save(_ draft: LocalConnectorModelProviderDraft) {
        if let editingProvider {
            viewModel.updateModelProvider(id: editingProvider.id, draft: draft)
        } else {
            viewModel.createModelProvider(draft)
        }
        closeEditor()
    }

    private func closeEditor() {
        editingProvider = nil
        isCreating = false
    }

    private func statusBadge(_ title: String, color: Color) -> some View {
        Text(title)
            .appFont(.caption2.weight(.medium))
            .foregroundStyle(color)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(color.opacity(0.1), in: Capsule())
    }

    private func capabilityBadge(_ title: String, enabled: Bool) -> some View {
        Label(title, systemImage: enabled ? "checkmark.circle.fill" : "circle")
            .appFont(.caption2)
            .foregroundStyle(enabled ? .secondary : .tertiary)
    }
}

private struct LocalConnectorProviderEditor: View {
    @EnvironmentObject private var appModel: AppModel
    var provider: LocalConnectorModelProvider?
    var disabled: Bool
    var onCancel: () -> Void
    var onSave: (LocalConnectorModelProviderDraft) -> Void

    @State private var name: String
    @State private var providerType: String
    @State private var promptVendor: String
    @State private var baseURL: String
    @State private var apiKey = ""
    @State private var clearAPIKey = false
    @State private var enabled: Bool
    @State private var supportsImages: Bool
    @State private var supportsReasoning: Bool
    @State private var supportsResponses: Bool
    @State private var validationMessage: String?

    private let providerOptions = ["gpt", "deepseek", "kimi", "glm"]
    private let promptVendorOptions = ["gpt", "deepseek", "kimi", "glm"]

    init(
        provider: LocalConnectorModelProvider?,
        disabled: Bool,
        onCancel: @escaping () -> Void,
        onSave: @escaping (LocalConnectorModelProviderDraft) -> Void
    ) {
        self.provider = provider
        self.disabled = disabled
        self.onCancel = onCancel
        self.onSave = onSave
        _name = State(initialValue: provider?.name ?? "")
        _providerType = State(initialValue: provider?.provider ?? "gpt")
        _promptVendor = State(initialValue: provider?.promptVendor ?? "gpt")
        _baseURL = State(initialValue: provider?.baseURL ?? "")
        _enabled = State(initialValue: provider?.enabled ?? true)
        _supportsImages = State(initialValue: provider?.supportsImages ?? false)
        _supportsReasoning = State(initialValue: provider?.supportsReasoning ?? false)
        _supportsResponses = State(initialValue: provider?.supportsResponses ?? false)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                if let validationMessage {
                    Label(validationMessage, systemImage: "exclamationmark.triangle.fill")
                        .appFont(.callout)
                        .foregroundStyle(.orange)
                }
                GroupBox(appModel.localized("连接信息", english: "Connection")) {
                    Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 14) {
                        editorRow(appModel.localized("名称", english: "Name")) {
                            TextField(appModel.localized("例如：OpenAI Production", english: "For example: OpenAI Production"), text: $name)
                        }
                        editorRow(appModel.localized("供应商协议", english: "Provider protocol")) {
                            Picker(appModel.localized("供应商协议", english: "Provider protocol"), selection: $providerType) {
                                ForEach(providerOptions, id: \.self, content: Text.init)
                            }
                            .labelsHidden()
                            .onChange(of: providerType) { _, next in
                                promptVendor = defaultPromptVendor(next)
                            }
                        }
                        editorRow(appModel.localized("Prompt 模板", english: "Prompt template")) {
                            Picker(appModel.localized("Prompt 模板", english: "Prompt template"), selection: $promptVendor) {
                                ForEach(promptVendorOptions, id: \.self, content: Text.init)
                            }
                            .labelsHidden()
                        }
                        editorRow("Base URL") {
                            TextField("https://api.example.com/v1", text: $baseURL)
                        }
                        editorRow("API Key") {
                            SecureField(
                                provider == nil
                                    ? appModel.localized("必填", english: "Required")
                                    : appModel.localized("留空则保留现有密钥", english: "Leave blank to keep the existing key"),
                                text: $apiKey
                            )
                                .disabled(clearAPIKey)
                        }
                    }
                    .textFieldStyle(.roundedBorder)
                    .padding(.top, 8)
                    if provider?.hasAPIKey == true {
                        Toggle(appModel.localized(
                            "删除服务器中已保存的 API Key",
                            english: "Delete the API key saved on the server"
                        ), isOn: $clearAPIKey)
                            .foregroundStyle(clearAPIKey ? .red : .secondary)
                            .padding(.top, 10)
                    }
                }

                GroupBox(appModel.localized("能力与状态", english: "Capabilities & Status")) {
                    VStack(alignment: .leading, spacing: 12) {
                        Toggle(appModel.localized("启用这个供应商", english: "Enable this provider"), isOn: $enabled)
                        Toggle(appModel.localized("支持图片输入", english: "Supports image input"), isOn: $supportsImages)
                        Toggle(appModel.localized("支持推理模型", english: "Supports reasoning models"), isOn: $supportsReasoning)
                        Toggle(appModel.localized("支持 Responses API", english: "Supports Responses API"), isOn: $supportsResponses)
                    }
                    .padding(.top, 8)
                }

                HStack {
                    Spacer()
                    Button(appModel.localized("取消", english: "Cancel"), action: onCancel)
                    Button(
                        provider == nil
                            ? appModel.localized("添加供应商", english: "Add Provider")
                            : appModel.localized("保存修改", english: "Save Changes"),
                        action: submit
                    )
                        .buttonStyle(.borderedProminent)
                        .disabled(disabled)
                }
            }
            .padding(20)
        }
    }

    private func editorRow<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        GridRow {
            Text(title)
                .appFont(.callout)
                .foregroundStyle(.secondary)
                .frame(width: 110, alignment: .trailing)
            content()
                .frame(maxWidth: .infinity)
        }
    }

    private func submit() {
        let cleanName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let cleanBaseURL = baseURL.trimmingCharacters(in: .whitespacesAndNewlines)
        let cleanAPIKey = apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanName.isEmpty, URL(string: cleanBaseURL)?.scheme != nil else {
            validationMessage = appModel.localized(
                "请填写供应商名称和有效的 Base URL。",
                english: "Enter a provider name and a valid Base URL."
            )
            return
        }
        if provider == nil && cleanAPIKey.isEmpty {
            validationMessage = appModel.localized(
                "新增供应商时必须填写 API Key。",
                english: "An API key is required when adding a provider."
            )
            return
        }
        validationMessage = nil
        onSave(.init(
            name: cleanName,
            provider: providerType,
            promptVendor: promptVendor,
            baseURL: cleanBaseURL,
            apiKey: cleanAPIKey,
            clearAPIKey: clearAPIKey,
            enabled: enabled,
            supportsImages: supportsImages,
            supportsReasoning: supportsReasoning,
            supportsResponses: supportsResponses
        ))
    }

    private func defaultPromptVendor(_ value: String) -> String {
        switch value.lowercased() {
        case "deepseek": "deepseek"
        case "kimi", "moonshot", "kimik2": "kimi"
        case "glm", "zhipu", "zai": "glm"
        default: "gpt"
        }
    }
}
