import ChatOSCore
import SwiftUI

/// Immutable card inputs isolate editor preparation and unrelated workspace updates.
struct AgentManagementCard: View, Equatable {
    let agent: LocalAgentProfile
    let modelAvailability: AgentModelAvailability
    let professionLabel: String
    let isSelected: Bool
    let isPreparingEditor: Bool
    let onOpenDirect: @MainActor () -> Void
    let onEdit: @MainActor () -> Void
    let onSelect: @MainActor () -> Void

    nonisolated static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.agent == rhs.agent && lhs.modelAvailability == rhs.modelAvailability
            && lhs.professionLabel == rhs.professionLabel && lhs.isSelected == rhs.isSelected
            && lhs.isPreparingEditor == rhs.isPreparingEditor
    }

    var body: some View {
        let canManageStaff = LocalAgentPermission.canManageStaff(agent.draft.defaultSkillIDs)
        let canAccessLocalProjects = LocalAgentPermission.canAccessLocalProjects(
            agent.draft.defaultSkillIDs
        )
        return VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top, spacing: 10) {
                AgentAvatarView(
                    name: agent.draft.name,
                    data: agent.draft.avatarData,
                    size: AgentAvatarMetrics.managementCard,
                    cornerRadius: 26
                )
                VStack(alignment: .leading, spacing: 3) {
                    Text(agent.draft.name)
                        .font(.headline)
                    Text(canManageStaff ? "可招募和解雇成员" : "无人员管理权限")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }
            if !agent.draft.description.isEmpty {
                Text(agent.draft.description)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            HStack(spacing: 10) {
                Button("私聊", systemImage: "bubble.left.and.bubble.right") {
                    onOpenDirect()
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
                .fixedSize()
                Spacer(minLength: 0)
                Button {
                    onEdit()
                } label: {
                    Text("编辑")
                        .opacity(isPreparingEditor ? 0 : 1)
                        .overlay {
                            if isPreparingEditor {
                                ProgressView().controlSize(.small)
                            }
                        }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .fixedSize()
                .disabled(isPreparingEditor)
            }
            Divider()
            LabeledContent("模型") {
                AgentModelStatusLabel(availability: modelAvailability)
            }
            .font(.caption)
            LabeledContent("思考等级") {
                Text(agent.draft.thinkingLevel ?? "跟随模型默认")
            }
            .font(.caption)
            LabeledContent("职业") {
                Text(professionLabel)
            }
            .font(.caption)
            LabeledContent("主动巡检") {
                Text(
                    agent.draft.heartbeatEnabled
                        ? Self.heartbeatIntervalLabel(agent.draft.heartbeatIntervalSeconds)
                        : "关闭"
                )
            }
            .font(.caption)
            if canManageStaff || canAccessLocalProjects {
                HStack(spacing: 6) {
                    if canManageStaff {
                        Label("人员管理", systemImage: "person.2.badge.gearshape")
                    }
                    if canAccessLocalProjects {
                        Label("项目与团队", systemImage: "folder.badge.gearshape")
                    }
                }
                .font(.caption2)
                .foregroundStyle(.secondary)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 12))
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .stroke(
                    isSelected
                        ? Color.accentColor : Color.primary.opacity(0.08),
                    lineWidth: isSelected ? 2 : 1
                )
        }
        .contentShape(RoundedRectangle(cornerRadius: 12))
        .onTapGesture {
            onSelect()
        }
    }

    private static func heartbeatIntervalLabel(_ seconds: Int) -> String {
        switch seconds {
        case 60: "每分钟"
        case 300: "每 5 分钟"
        case 900: "每 15 分钟"
        case 1_800: "每 30 分钟"
        case 3_600: "每小时"
        default: "每 \(seconds / 60) 分钟"
        }
    }
}
