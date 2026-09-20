import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import SwiftUI

extension ProjectAgentGroupChatView {
    var memberSidebar: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("成员").appFont(.headline)
                Spacer()
                Text("\(viewModel.members.count)")
                    .appFont(.caption).foregroundStyle(.secondary)
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
                        Task {
                            if await viewModel.prepareAgentEditor() {
                                editingMember = item
                            }
                        }
                    } label: {
                        if viewModel.isLoadingModels {
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
                    .disabled(viewModel.isLoadingModels)
                }
                .padding(9)
                .background(
                    selectedRunAgentID == item.member.agentID && selectedSection == .runs
                        ? Color.accentColor.opacity(0.12)
                        : Color.clear,
                    in: RoundedRectangle(cornerRadius: 9)
                )
                .overlay {
                    if selectedRunAgentID == item.member.agentID && selectedSection == .runs {
                        RoundedRectangle(cornerRadius: 9)
                            .stroke(Color.accentColor.opacity(0.35), lineWidth: 1)
                    }
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
        .padding(14)
        .background(Color(nsColor: .controlBackgroundColor))
        .onChange(of: viewModel.activeMembers.map(\.member.agentID)) { _, agentIDs in
            guard let selectedRunAgentID, !agentIDs.contains(selectedRunAgentID) else { return }
            self.selectedRunAgentID = nil
        }
    }

    private func memberSummary(_ item: AgentGroupChatViewModel.MemberPresentation) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack {
                Image(systemName: "person.crop.circle.fill")
                    .foregroundStyle(.tint)
                Text(item.profile?.draft.name ?? item.member.agentID)
                    .appFont(.body).fontWeight(.medium)
                Spacer()
                Image(systemName: "waveform.path.ecg")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            Text(item.member.draft.role)
                .appFont(.caption).foregroundStyle(.secondary)
            HStack(spacing: 7) {
                if viewModel.room?.defaultAgentID == item.member.agentID {
                    Text("默认 Agent")
                        .appFont(.caption2).foregroundStyle(.tint)
                }
                if viewModel.room?.projectManagerAgentID == item.member.agentID {
                    Text("项目经理")
                        .appFont(.caption2).foregroundStyle(.green)
                }
            }
        }
    }
}
