import ChatOSCore
import SwiftUI

struct ProjectRequirementSurveysView: View {
    @Environment(\.dismiss) private var dismiss
    let surveys: [LocalAgentRequirementSurvey]
    let submittingSurveyIDs: Set<String>
    let creatorNamesByID: [String: String]
    var projectNamesByID: [String: String] = [:]
    var showsNavigationBackButton = false
    var heading = "需求调研"
    var explanation = "项目经理可在新需求、重大变更或任何信息不足的节点发起调研。选择答案后，页面末尾可统一补充备注。"
    let onSubmit: (
        LocalAgentRequirementSurvey,
        [String: [String]],
        String
    ) async -> Bool
    @State private var selectedSurveyID: String?
    @State private var page = 0
    @State private var pageSize = 20

    private var pagedSurveys: [LocalAgentRequirementSurvey] {
        surveys.agentPage(index: page, size: pageSize)
    }

    var body: some View {
        if let selectedSurvey = surveys.first(where: { $0.id == selectedSurveyID }) {
            RequirementSurveyDetailView(
                survey: selectedSurvey,
                creatorName: creatorNamesByID[selectedSurvey.creatorAgentID] ?? "项目经理",
                projectName: projectNamesByID[selectedSurvey.projectID],
                isSubmitting: submittingSurveyIDs.contains(selectedSurvey.id),
                onBack: { selectedSurveyID = nil },
                onSubmit: onSubmit
            )
        } else {
            surveyList
        }
    }

    private var surveyList: some View {
        ZStack {
            LinearGradient(
                colors: [
                    Color(nsColor: .windowBackgroundColor),
                    AppPalette.ai.opacity(0.035),
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            .ignoresSafeArea()

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18) {
                    overviewHeader

                    if surveys.isEmpty {
                        ContentUnavailableView {
                            Label("暂无需求调研", systemImage: "list.clipboard")
                        } description: {
                            Text("项目经理需要确认目标、范围或方案时，会在这里创建调研单。")
                        }
                        .frame(maxWidth: .infinity, minHeight: 320)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18))
                    } else {
                        let pending = pagedSurveys.filter { $0.status == .pending }
                        let awaitingResolution = pagedSurveys.filter {
                            $0.status == .submitted && $0.resolution == nil
                        }
                        let resolved = pagedSurveys.filter { $0.resolution != nil }
                        if !pending.isEmpty {
                            sectionTitle("待填写", count: pending.count)
                            surveyGrid(pending)
                        }
                        if !awaitingResolution.isEmpty {
                            sectionTitle("等待形成方案", count: awaitingResolution.count)
                                .padding(.top, pending.isEmpty ? 0 : 8)
                            surveyGrid(awaitingResolution)
                        }
                        if !resolved.isEmpty {
                            sectionTitle("已形成方案", count: resolved.count)
                                .padding(.top, pending.isEmpty && awaitingResolution.isEmpty ? 0 : 8)
                            surveyGrid(resolved)
                        }
                        AgentListPaginationBar(
                            totalCount: surveys.count,
                            page: $page,
                            pageSize: $pageSize
                        )
                    }
                }
                .padding(24)
                .frame(maxWidth: 1380, alignment: .leading)
                .frame(maxWidth: .infinity)
            }
        }
    }

    private var overviewHeader: some View {
        let pending = surveys.filter { $0.status == .pending }.count
        let waiting = surveys.filter { $0.status == .submitted && $0.resolution == nil }.count
        let resolved = surveys.filter { $0.resolution != nil }.count

        return VStack(alignment: .leading, spacing: 18) {
            HStack(alignment: .top, spacing: 16) {
                if showsNavigationBackButton {
                    Button {
                        dismiss()
                    } label: {
                        Label("返回项目列表", systemImage: "chevron.left")
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .help("返回需求调研项目列表")
                }

                ZStack {
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .fill(
                            LinearGradient(
                                colors: [AppPalette.ai, Color.indigo],
                                startPoint: .topLeading,
                                endPoint: .bottomTrailing
                            )
                        )
                    Image(systemName: "text.badge.checkmark")
                        .font(.system(size: 23, weight: .semibold))
                        .foregroundStyle(.white)
                }
                .frame(width: 54, height: 54)
                .shadow(color: AppPalette.ai.opacity(0.2), radius: 9, y: 4)

                VStack(alignment: .leading, spacing: 6) {
                    Text(heading)
                        .appFont(.title2.weight(.bold))
                    Text(explanation)
                        .appFont(.body)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 16)
            }

            HStack(spacing: 10) {
                overviewMetric(
                    value: surveys.count,
                    title: "全部调研",
                    systemImage: "rectangle.stack.fill",
                    color: .blue
                )
                overviewMetric(
                    value: pending,
                    title: "待你填写",
                    systemImage: "square.and.pencil",
                    color: AppPalette.ai
                )
                overviewMetric(
                    value: waiting,
                    title: "方案生成中",
                    systemImage: "hourglass",
                    color: .orange
                )
                overviewMetric(
                    value: resolved,
                    title: "已有方案",
                    systemImage: "checkmark.seal.fill",
                    color: .green
                )
            }
        }
        .padding(22)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(AppPalette.ai.opacity(0.13), lineWidth: 1)
        }
        .shadow(color: .black.opacity(0.035), radius: 12, y: 5)
    }

    private func overviewMetric(
        value: Int,
        title: String,
        systemImage: String,
        color: Color
    ) -> some View {
        HStack(spacing: 9) {
            Image(systemName: systemImage)
                .foregroundStyle(color)
                .frame(width: 22)
            VStack(alignment: .leading, spacing: 1) {
                Text("\(value)")
                    .appFont(.headline.weight(.bold))
                    .monospacedDigit()
                Text(title)
                    .appFont(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(color.opacity(0.07), in: RoundedRectangle(cornerRadius: 10))
    }

    private func sectionTitle(_ title: String, count: Int) -> some View {
        let presentation: (icon: String, color: Color) = switch title {
        case "待填写": ("square.and.pencil", AppPalette.ai)
        case "等待形成方案": ("hourglass", .orange)
        default: ("checkmark.seal.fill", .green)
        }

        return HStack(spacing: 8) {
            Image(systemName: presentation.icon)
                .foregroundStyle(presentation.color)
            Text(title).appFont(.headline.weight(.semibold))
            Text("\(count)")
                .appFont(.caption2)
                .foregroundStyle(presentation.color)
                .padding(.horizontal, 7)
                .padding(.vertical, 2)
                .background(presentation.color.opacity(0.1), in: Capsule())
            Spacer()
        }
    }

    private func surveyLink(
        survey: LocalAgentRequirementSurvey,
        creatorName: String
    ) -> some View {
        Button {
            selectedSurveyID = survey.id
        } label: {
            RequirementSurveyRow(
                survey: survey,
                creatorName: creatorName,
                projectName: projectNamesByID[survey.projectID]
            )
        }
        .buttonStyle(.plain)
    }

    private func surveyGrid(
        _ items: [LocalAgentRequirementSurvey]
    ) -> some View {
        LazyVGrid(
            columns: [
                GridItem(
                    .adaptive(minimum: 410, maximum: 680),
                    spacing: 14,
                    alignment: .top
                ),
            ],
            alignment: .leading,
            spacing: 14
        ) {
            ForEach(items) { survey in
                surveyLink(
                    survey: survey,
                    creatorName: creatorNamesByID[survey.creatorAgentID] ?? "项目经理"
                )
            }
        }
    }
}
