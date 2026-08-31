import ChatOSCore
import SwiftUI

struct LocalConnectorApprovalsView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: LocalConnectorControlCenterViewModel
    @State private var proposedElevatedMode: LocalConnectorApprovalMode?

    var body: some View {
        SettingsGroupedPage {
            policyCard
            pendingCard
            historyCard
        }
        .confirmationDialog(
            approvalConfirmationTitle,
            isPresented: Binding(
                get: { proposedElevatedMode != nil },
                set: { if !$0 { proposedElevatedMode = nil } }
            ),
            titleVisibility: .visible
        ) {
            if let mode = proposedElevatedMode {
                Button(approvalConfirmationButton(for: mode), role: .destructive) {
                    proposedElevatedMode = nil
                    viewModel.updateApprovalMode(mode, riskAcknowledged: true)
                }
            }
            Button(model.localized("取消", english: "Cancel"), role: .cancel) {
                proposedElevatedMode = nil
            }
        } message: {
            Text(approvalConfirmationMessage)
        }
    }

    private var policyCard: some View {
        LocalConnectorCard(
            model.localized("默认审批级别", english: "Default Approval Level"),
            subtitle: model.localized(
                "项目没有单独策略时使用这里的默认值",
                english: "Projects use this default unless they define their own policy"
            ),
            systemImage: "checkmark.shield"
        ) {
            Picker(
                model.localized("审批模式", english: "Approval mode"),
                selection: Binding(
                    get: { viewModel.approvalSettings?.defaultMode ?? .requestApproval },
                    set: { mode in
                        if mode == .requestApproval {
                            viewModel.updateApprovalMode(mode, riskAcknowledged: false)
                        } else {
                            proposedElevatedMode = mode
                        }
                    }
                )
            ) {
                Text(model.localized("需要确认", english: "Ask Every Time")).tag(LocalConnectorApprovalMode.requestApproval)
                Text(model.localized("自动审批", english: "Automatic")).tag(LocalConnectorApprovalMode.autoApproval)
                Text(model.localized("完全控制", english: "Full Control")).tag(LocalConnectorApprovalMode.fullControl)
            }
            .pickerStyle(.segmented)
            Text(approvalModeDescription)
                .appFont(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var pendingCard: some View {
        LocalConnectorCard(
            model.localized("等待处理", english: "Pending"),
            subtitle: model.localized(
                "任务会停在这里，直到用户或审批模型作出决定",
                english: "Tasks pause here until the user or approval model decides"
            ),
            systemImage: "hourglass"
        ) {
            if viewModel.pendingApprovals.isEmpty {
                Label(model.localized("当前没有等待审批的操作", english: "No operations are waiting for approval"), systemImage: "checkmark.circle")
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, minHeight: 70)
            } else {
                VStack(spacing: 0) {
                    ForEach(viewModel.pendingApprovals) { approval in
                        pendingRow(approval)
                        if approval.id != viewModel.pendingApprovals.last?.id { Divider() }
                    }
                }
            }
        }
    }

    private func pendingRow(_ approval: LocalConnectorPendingApproval) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(approval.risk.uppercased())
                    .appFont(.caption2.weight(.bold))
                    .foregroundStyle(riskColor(approval.risk))
                    .padding(.horizontal, 7)
                    .padding(.vertical, 3)
                    .background(riskColor(approval.risk).opacity(0.1), in: Capsule())
                Text(approval.source)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Text(approval.createdAt)
                    .appFont(.caption2)
                    .foregroundStyle(.tertiary)
            }
            Text(approval.command)
                .appFont(.system(.callout, design: .monospaced))
                .textSelection(.enabled)
            Text(approval.cwd)
                .appFont(.caption.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            if let reason = approval.reason, !reason.isEmpty {
                Text(reason)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
            HStack {
                Button(model.localized("拒绝", english: "Decline"), role: .destructive) {
                    viewModel.resolveApproval(id: approval.id, decision: "decline")
                }
                Spacer()
                Button(model.localized("仅本次允许", english: "Allow Once")) {
                    viewModel.resolveApproval(id: approval.id, decision: "accept")
                }
                Button(model.localized("本会话允许", english: "Allow for Session")) {
                    viewModel.resolveApproval(id: approval.id, decision: "acceptForSession")
                }
                .buttonStyle(.borderedProminent)
            }
        }
        .padding(.vertical, 10)
    }

    private var historyCard: some View {
        LocalConnectorCard(
            model.localized("审批记录", english: "Approval History"),
            subtitle: model.localized(
                "最近的命令和 Computer Use 决策审计",
                english: "Recent command and Computer Use decision audit"
            ),
            systemImage: "clock.arrow.circlepath"
        ) {
            let history = Array((viewModel.approvalSettings?.history ?? []).prefix(30))
            if history.isEmpty {
                Text(model.localized("还没有审批记录。", english: "No approval history yet."))
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, minHeight: 60)
            } else {
                VStack(spacing: 0) {
                    ForEach(history) { entry in
                        HStack(alignment: .top, spacing: 12) {
                            Image(systemName: decisionIcon(entry.decision))
                                .foregroundStyle(decisionColor(entry.decision))
                                .frame(width: 18)
                            VStack(alignment: .leading, spacing: 4) {
                                Text(entry.command)
                                    .appFont(.system(.callout, design: .monospaced))
                                    .lineLimit(2)
                                Text("\(entry.source) · \(entry.createdAt)")
                                    .appFont(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Text(entry.decision)
                                .appFont(.caption.weight(.medium))
                                .foregroundStyle(decisionColor(entry.decision))
                        }
                        .padding(.vertical, 8)
                        if entry.id != history.last?.id { Divider() }
                    }
                }
            }
        }
    }

    private var approvalModeDescription: String {
        switch viewModel.approvalSettings?.defaultMode ?? .requestApproval {
        case .requestApproval:
            model.localized(
                "敏感操作逐条确认；这是默认推荐策略。",
                english: "Confirm each sensitive operation. This is the recommended default."
            )
        case .autoApproval:
            model.localized(
                "由审批模型根据操作风险自动决定，无法判断时仍会询问用户。",
                english: "The approval model decides based on risk and asks you when uncertain."
            )
        case .fullControl:
            model.localized(
                "允许任务直接执行高风险操作，只适合完全受信任的环境。",
                english: "Tasks may execute high-risk operations directly. Use only in fully trusted environments."
            )
        }
    }

    private var approvalConfirmationTitle: String {
        switch proposedElevatedMode {
        case .autoApproval: model.localized("确认启用自动审批？", english: "Enable automatic approval?")
        case .fullControl: model.localized("确认授予完全控制？", english: "Grant full control?")
        default: model.localized("确认更改审批模式？", english: "Change approval mode?")
        }
    }

    private var approvalConfirmationMessage: String {
        switch proposedElevatedMode {
        case .autoApproval:
            model.localized(
                "审批模型将自动决定部分敏感操作，无法判断时仍会请求你的确认。",
                english: "The approval model will decide some sensitive operations automatically and ask you when uncertain."
            )
        case .fullControl:
            model.localized(
                "任务将可以直接执行高风险操作。请只在完全信任当前设备、项目与任务时使用。",
                english: "Tasks will be able to execute high-risk operations directly. Use this only when you fully trust the device, project, and task."
            )
        default:
            model.localized(
                "此操作会改变本机任务的默认审批策略。",
                english: "This changes the default approval policy for local tasks."
            )
        }
    }

    private func approvalConfirmationButton(for mode: LocalConnectorApprovalMode) -> String {
        switch mode {
        case .autoApproval: model.localized("启用自动审批", english: "Enable Automatic Approval")
        case .fullControl: model.localized("授予完全控制", english: "Grant Full Control")
        case .requestApproval: model.localized("切换", english: "Switch")
        }
    }

    private func riskColor(_ risk: String) -> Color {
        switch risk.lowercased() {
        case "critical", "high": .red
        case "medium": .orange
        default: .blue
        }
    }

    private func decisionIcon(_ decision: String) -> String {
        decision.lowercased().contains("allow") || decision.lowercased().contains("accept")
            ? "checkmark.circle.fill" : "xmark.circle.fill"
    }

    private func decisionColor(_ decision: String) -> Color {
        decision.lowercased().contains("allow") || decision.lowercased().contains("accept")
            ? .green : .orange
    }
}
