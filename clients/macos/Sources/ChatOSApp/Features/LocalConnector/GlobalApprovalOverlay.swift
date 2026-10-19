import ChatOSCore
import SwiftUI

struct GlobalApprovalOverlayHost: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.openSettings) private var openSettings
    @ObservedObject var viewModel: LocalConnectorControlCenterViewModel
    @State private var queueState = LocalConnectorApprovalQueuePresentationState()

    var body: some View {
        if !viewModel.pendingApprovals.isEmpty {
            Group {
                if queueState.isExpanded {
                    expandedCard
                } else {
                    compactButton
                }
            }
            .onAppear {
                queueState.appear(approvalIDs: approvalIDs)
            }
            .onChange(of: approvalIDs) { _, nextIDs in
                queueState.update(approvalIDs: nextIDs)
            }
            .transition(.move(edge: .trailing).combined(with: .opacity))
            .animation(.easeInOut(duration: 0.2), value: queueState.isExpanded)
            .animation(.easeInOut(duration: 0.2), value: approvalIDs)
        }
    }

    private var compactButton: some View {
        Button {
            queueState.expand()
        } label: {
            HStack(spacing: 10) {
                Image(systemName: "bell.badge.fill")
                    .foregroundStyle(.orange)
                    .frame(width: 28, height: 28)
                    .background(Color.orange.opacity(0.12), in: RoundedRectangle(cornerRadius: 8))
                VStack(alignment: .leading, spacing: 2) {
                    Text(model.localized(
                        "\(viewModel.pendingApprovals.count) 项等待你的审批",
                        english: "\(viewModel.pendingApprovals.count) awaiting your approval"
                    ))
                        .appFont(.caption.weight(.semibold))
                    Text(model.localized("点击展开并处理", english: "Click to review and decide"))
                        .appFont(.caption2)
                        .foregroundStyle(.secondary)
                }
                Image(systemName: "chevron.left")
                    .appFont(.caption2)
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 11)
            .frame(height: 48)
        }
        .buttonStyle(.plain)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .stroke(Color.orange.opacity(0.35), lineWidth: 1)
        }
        .shadow(radius: 14, y: 6)
        .accessibilityLabel(model.localized(
            "\(viewModel.pendingApprovals.count) 项等待审批，点击展开",
            english: "\(viewModel.pendingApprovals.count) approvals pending. Click to expand."
        ))
    }

    private var expandedCard: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider()
            approvalContent
        }
        .frame(width: 420)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14)
                .stroke(.primary.opacity(0.14), lineWidth: 1)
        }
        .shadow(radius: 20, y: 8)
    }

    private var header: some View {
        HStack(spacing: 10) {
            Image(systemName: "bell.badge.fill")
                .foregroundStyle(.orange)
            VStack(alignment: .leading, spacing: 2) {
                Text(model.localized("命令审批", english: "Command Approval"))
                    .appFont(.callout.weight(.semibold))
                Text(model.localized(
                    "\(viewModel.pendingApprovals.count) 项等待处理",
                    english: "\(viewModel.pendingApprovals.count) pending"
                ))
                    .appFont(.caption2)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button(model.localized("收起", english: "Collapse"), systemImage: "chevron.right") {
                queueState.collapse()
            }
            .labelStyle(.iconOnly)
            .buttonStyle(.plain)
            .help(model.localized("收起审批浮层", english: "Collapse approval panel"))
            Button(model.localized("打开完整审批页", english: "Open Approval Settings"), systemImage: "arrow.up.right.square") {
                model.requestConnectorSettings(.approvals)
                openSettings()
            }
            .labelStyle(.iconOnly)
            .buttonStyle(.plain)
            .help(model.localized("打开完整审批页", english: "Open Approval Settings"))
        }
        .padding(13)
    }

    @ViewBuilder
    private var approvalContent: some View {
        if let approval = activeApproval {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    HStack(spacing: 8) {
                        Text(riskLabel(approval.risk))
                            .appFont(.caption2.weight(.bold))
                            .foregroundStyle(riskColor(approval.risk))
                            .padding(.horizontal, 7)
                            .padding(.vertical, 3)
                            .background(riskColor(approval.risk).opacity(0.11), in: Capsule())
                        Text(approval.source)
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                        Spacer()
                        Text(approvalDate(approval.createdAt))
                            .appFont(.caption2.monospacedDigit())
                            .foregroundStyle(.tertiary)
                    }

                    if let purpose = approval.purpose?.trimmingCharacters(in: .whitespacesAndNewlines),
                       !purpose.isEmpty {
                        VStack(alignment: .leading, spacing: 4) {
                            Label(model.localized("运行目的", english: "Purpose"), systemImage: "target")
                                .appFont(.caption.weight(.semibold))
                                .foregroundStyle(.secondary)
                            Text(purpose)
                                .appFont(.callout.weight(.semibold))
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }

                    DisclosureGroup(model.localized("命令详情", english: "Command details")) {
                        Text(approval.command)
                            .appFont(.system(.caption, design: .monospaced))
                            .textSelection(.enabled)
                            .fixedSize(horizontal: false, vertical: true)
                            .padding(.top, 4)
                    }
                    .appFont(.caption)

                    Label(approval.cwd, systemImage: "folder")
                        .appFont(.caption.monospaced())
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                        .textSelection(.enabled)

                    if let reason = approval.reason, !reason.isEmpty {
                        Text(reason)
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    if let error = viewModel.approvalError(for: approval.id) {
                        Label(error, systemImage: "exclamationmark.triangle.fill")
                            .appFont(.caption)
                            .foregroundStyle(.orange)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                .padding(14)
            }
            .frame(maxHeight: 300)

            Divider()

            HStack(spacing: 10) {
                if viewModel.pendingApprovals.count > 1 {
                    Button(model.localized("上一条", english: "Previous"), systemImage: "chevron.left") {
                        queueState.moveToPrevious()
                    }
                    .labelStyle(.iconOnly)
                    .disabled(queueState.activeIndex == 0)
                    Text("\(queueState.activeIndex + 1) / \(viewModel.pendingApprovals.count)")
                        .appFont(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                    Button(model.localized("下一条", english: "Next"), systemImage: "chevron.right") {
                        queueState.moveToNext(
                            approvalCount: viewModel.pendingApprovals.count
                        )
                    }
                    .labelStyle(.iconOnly)
                    .disabled(
                        queueState.activeIndex >= viewModel.pendingApprovals.count - 1
                    )
                }

                Spacer()

                if viewModel.isResolvingApproval(approval.id) {
                    ProgressView().controlSize(.small)
                }

                if approval.allows(.decline) {
                    Button(model.localized("拒绝", english: "Decline"), role: .destructive) {
                        viewModel.resolveApproval(approval, decision: .decline)
                    }
                    .disabled(viewModel.isResolvingApproval(approval.id))
                }
                if approval.allows(.accept) {
                    Button(model.localized("仅本次允许", english: "Allow Once")) {
                        viewModel.resolveApproval(approval, decision: .accept)
                    }
                    .disabled(viewModel.isResolvingApproval(approval.id))
                }
                if approval.allows(.acceptForSession) {
                    Button(model.localized("本会话允许", english: "Allow for Session")) {
                        viewModel.resolveApproval(approval, decision: .acceptForSession)
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(viewModel.isResolvingApproval(approval.id))
                }
            }
            .controlSize(.small)
            .padding(12)
        }
    }

    private var approvalIDs: [String] {
        viewModel.pendingApprovals.map(\.id)
    }

    private var activeApproval: LocalConnectorPendingApproval? {
        queueState.activeApproval(in: viewModel.pendingApprovals)
    }

    private func riskLabel(_ risk: String) -> String {
        switch risk.lowercased() {
        case "high", "critical": model.localized("高风险", english: "High Risk")
        case "medium": model.localized("中风险", english: "Medium Risk")
        default: model.localized("低风险", english: "Low Risk")
        }
    }

    private func riskColor(_ risk: String) -> Color {
        switch risk.lowercased() {
        case "high", "critical": .red
        case "medium": .orange
        default: .green
        }
    }

    private func approvalDate(_ value: String) -> String {
        guard let date = ISO8601DateFormatter().date(from: value) else { return value }
        return date.formatted(date: .omitted, time: .shortened)
    }
}
