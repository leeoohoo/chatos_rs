import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import SwiftUI

extension ProjectAgentGroupChatView {
    var memberSidebar: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("团队成员").appFont(.headline.weight(.semibold))
                    Text("点击成员查看运行状态")
                        .appFont(.caption2)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Text("\(viewModel.members.count)")
                    .appFont(.caption.weight(.semibold))
                    .foregroundStyle(AppPalette.ai)
                    .frame(minWidth: 24, minHeight: 24)
                    .background(AppPalette.aiSoft, in: Capsule())
            }
            if viewModel.activeMembers.isEmpty {
                Text("还没有 Agent。创建第一个成员后，它会成为默认 Agent。")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
            if !viewModel.activeMembers.isEmpty,
               viewModel.room?.projectManagerAgentID == nil {
                Label("尚未指定项目经理，团队任务板暂不可创建任务。", systemImage: "exclamationmark.triangle")
                    .appFont(.caption)
                    .foregroundStyle(.orange)
            }
            ForEach(viewModel.activeMembers) { item in
                HStack(spacing: 8) {
                    Button {
                        selectedRunAgentID = item.member.agentID
                        selectedSection = .runs
                    } label: {
                        memberSummary(item)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .help("查看此 Agent 的运行情况")

                    Button {
                        editingMember = item
                        preparingMemberEditorAgentID = item.member.agentID
                        Task {
                            let isReady = await viewModel.prepareAgentEditor()
                            guard editingMember?.member.agentID == item.member.agentID else { return }
                            if isReady {
                                preparingMemberEditorAgentID = nil
                            } else {
                                editingMember = nil
                                preparingMemberEditorAgentID = nil
                            }
                        }
                    } label: {
                        if preparingMemberEditorAgentID == item.member.agentID {
                            ProgressView()
                                .controlSize(.small)
                                .frame(width: 24, height: 24)
                        } else {
                            Image(systemName: "slider.horizontal.3")
                                .frame(width: 24, height: 24)
                        }
                    }
                    .buttonStyle(.borderless)
                    .help("编辑 Agent")
                    .disabled(preparingMemberEditorAgentID == item.member.agentID)
                    .popover(
                        isPresented: memberEditorPresentation(for: item.member.agentID),
                        attachmentAnchor: .rect(.bounds),
                        arrowEdge: .trailing
                    ) {
                        if preparingMemberEditorAgentID == item.member.agentID {
                            ProgressView("正在准备 Agent 设置…")
                                .frame(width: 320, height: 160)
                        } else {
                            EditLocalAgentSheet(viewModel: viewModel, item: item)
                        }
                    }
                }
                .padding(10)
                .background(
                    selectedRunAgentID == item.member.agentID && selectedSection == .runs
                        ? AppPalette.aiSoft
                        : AppPalette.surface,
                    in: RoundedRectangle(cornerRadius: 12)
                )
                .overlay {
                    RoundedRectangle(cornerRadius: 12)
                        .stroke(
                            selectedRunAgentID == item.member.agentID && selectedSection == .runs
                                ? AppPalette.ai.opacity(0.35)
                                : AppPalette.border.opacity(0.65),
                            lineWidth: 1
                        )
                }
            }
            Spacer()
            Button {
                showsAddExistingAgent = true
            } label: {
                Label("邀请 Agent", systemImage: "person.crop.circle.badge.plus")
            }
            .buttonStyle(.borderedProminent)
            .frame(maxWidth: .infinity)
            Menu {
                Button("手动创建", systemImage: "square.and.pencil") {
                    Task {
                        if await viewModel.prepareAgentEditor() {
                            showsCreateAgent = true
                        }
                    }
                }
                Button("Agent Builder", systemImage: "sparkles") {
                    Task {
                        if await viewModel.prepareAgentEditor() {
                            showsAgentBuilder = true
                        }
                    }
                }
            } label: {
                if viewModel.isLoadingModels {
                    ProgressView().controlSize(.small)
                } else {
                    Label("创建新 Agent", systemImage: "plus")
                }
            }
            .buttonStyle(.bordered)
            .frame(maxWidth: .infinity)
            .disabled(viewModel.isLoadingModels)
        }
        .padding(16)
        .background(AppPalette.surfaceSubtle)
        .onChange(of: viewModel.activeMembers.map(\.member.agentID)) { _, agentIDs in
            guard let selectedRunAgentID, !agentIDs.contains(selectedRunAgentID) else { return }
            self.selectedRunAgentID = nil
        }
    }

    private func memberSummary(_ item: AgentGroupChatViewModel.MemberPresentation) -> some View {
        HStack(alignment: .top, spacing: 10) {
            AgentAvatarView(
                name: item.profile?.draft.name ?? "A",
                data: item.profile?.draft.avatarData,
                size: AgentAvatarMetrics.message,
                cornerRadius: 20
            )

            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Text(item.profile?.draft.name ?? item.member.agentID)
                        .appFont(.body.weight(.medium))
                    Spacer()
                    Circle()
                        .fill(AppPalette.terminalGreen)
                        .frame(width: 7, height: 7)
                        .help("可用")
                }
                Text(item.member.draft.role)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                HStack(spacing: 6) {
                    if viewModel.room?.defaultAgentID == item.member.agentID {
                        memberBadge("默认", color: AppPalette.ai)
                    }
                    if viewModel.room?.projectManagerAgentID == item.member.agentID {
                        memberBadge("项目经理", color: AppPalette.terminalGreen)
                    }
                }
            }
        }
    }

    private func memberBadge(_ title: String, color: Color) -> some View {
        Text(title)
            .appFont(.caption2.weight(.medium))
            .foregroundStyle(color)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(color.opacity(0.09), in: Capsule())
    }

    private func memberEditorPresentation(for agentID: String) -> Binding<Bool> {
        Binding(
            get: { editingMember?.member.agentID == agentID },
            set: { isPresented in
                guard !isPresented, editingMember?.member.agentID == agentID else { return }
                editingMember = nil
                preparingMemberEditorAgentID = nil
            }
        )
    }
}
