import ChatOSCore
import SwiftUI

struct LocalConnectorModelsView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: LocalConnectorControlCenterViewModel
    @State private var settingsDraft = LocalConnectorModelSettings(
        modelRequestMaxRetries: 5,
        commandApprovalModelConfigID: nil,
        commandApprovalThinkingLevel: nil
    )
    @State private var taskDrafts: [String: LocalConnectorTaskModelDraft] = [:]
    @State private var showingProviderManager = false
    @State private var validationMessage: String?

    var body: some View {
        SettingsGroupedPage {
            actionBar
            if let validationMessage {
                LocalConnectorCard(
                    model.localized("配置检查", english: "Configuration Check"),
                    systemImage: "exclamationmark.triangle.fill"
                ) {
                    Label(validationMessage, systemImage: "exclamationmark.triangle.fill")
                        .appFont(.callout)
                        .foregroundStyle(.orange)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }

            if let catalog = viewModel.modelCatalog {
                LocalConnectorModelDefaultsSection(
                    models: catalog.items,
                    settings: $settingsDraft
                )
                retrySection
                LocalConnectorTaskModelsSection(
                    models: catalog.items,
                    drafts: $taskDrafts
                )
            } else {
                LocalConnectorCard(
                    model.localized("模型配置", english: "Model Configuration"),
                    systemImage: "brain.head.profile"
                ) {
                    ProgressView(model.localized(
                        "正在读取完整模型配置…",
                        english: "Loading the complete model configuration…"
                    ))
                        .frame(maxWidth: .infinity, minHeight: 260)
                }
            }
        }
        .sheet(isPresented: $showingProviderManager) {
            LocalConnectorProviderManagerSheet(viewModel: viewModel)
        }
        .onAppear { synchronizeDrafts(with: viewModel.modelCatalog) }
        .onChange(of: viewModel.modelCatalog) { _, catalog in
            synchronizeDrafts(with: catalog)
        }
    }

    private var actionBar: some View {
        LocalConnectorCard(
            model.localized("统一模型配置", english: "Unified Model Configuration"),
            subtitle: model.localized(
                "供应商与云端默认模型保存在 ChatOS；本机审批模型只保存在这台 Mac。",
                english: "Providers and cloud defaults are stored in ChatOS; the local approval model stays on this Mac."
            ),
            systemImage: "slider.horizontal.3"
        ) {
            HStack(spacing: 10) {
                Button(model.localized("管理供应商", english: "Manage Providers"), systemImage: "server.rack") {
                    showingProviderManager = true
                }
                Button(model.localized("同步", english: "Sync"), systemImage: "arrow.triangle.2.circlepath") {
                    viewModel.loadModels(refresh: true)
                }
                Spacer()
                Button(model.localized("保存全部", english: "Save All"), systemImage: "checkmark") { save() }
                    .buttonStyle(.borderedProminent)
                    .disabled(viewModel.isPerformingAction || viewModel.modelCatalog == nil)
            }
        }
    }

    private var retrySection: some View {
        LocalConnectorCard(
            model.localized("请求容错", english: "Request Resilience"),
            subtitle: model.localized(
                "网络波动、限流或上游暂时不可用时的最大重试次数。",
                english: "Maximum retries for network errors, rate limits, or temporary upstream failures."
            ),
            systemImage: "arrow.clockwise.circle"
        ) {
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text(model.localized(
                        "模型请求最大重试次数",
                        english: "Maximum model request retries"
                    )).appFont(.headline)
                    Text(model.localized(
                        "允许范围 0–10，默认 5。",
                        english: "Allowed range: 0–10. Default: 5."
                    ))
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Stepper(
                    value: Binding(
                        get: { settingsDraft.modelRequestMaxRetries ?? 5 },
                        set: { settingsDraft.modelRequestMaxRetries = min(10, max(0, $0)) }
                    ),
                    in: 0...10
                ) {
                    Text(model.localized(
                        "\(settingsDraft.modelRequestMaxRetries ?? 5) 次",
                        english: "\(settingsDraft.modelRequestMaxRetries ?? 5)"
                    ))
                        .appFont(.body.monospacedDigit())
                        .frame(width: 48, alignment: .trailing)
                }
                .fixedSize()
            }
        }
    }

    private func synchronizeDrafts(with catalog: LocalConnectorModelCatalog?) {
        guard let catalog else { return }
        settingsDraft = catalog.settings
        taskDrafts = Dictionary(uniqueKeysWithValues: catalog.items.map {
            ($0.id, LocalConnectorTaskModelDraft(model: $0))
        })
        validationMessage = nil
    }

    private func save() {
        guard let catalog = viewModel.modelCatalog else { return }
        do {
            let plan = try LocalConnectorModelSavePlan(
                settings: settingsDraft, models: catalog.items, drafts: taskDrafts
            )
            validationMessage = nil
            viewModel.saveModelConfiguration(settings: plan.settings, updates: plan.updates)
        } catch {
            validationMessage = error.localizedDescription
        }
    }

}
