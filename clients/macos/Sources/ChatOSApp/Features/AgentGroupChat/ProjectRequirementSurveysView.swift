import ChatOSConnector
import SwiftUI

struct ProjectRequirementSurveysView: View {
    @Environment(\.dismiss) private var dismiss
    let surveys: [LocalAgentHostRequirementSurvey]
    let submittingSurveyIDs: Set<String>
    var showsNavigationBackButton = false
    var heading = "需求调研"
    var explanation = "本地任务需要补充信息时会暂停并创建调研单。"
    let onSubmit: (
        LocalAgentHostRequirementSurvey,
        [String: LocalAgentJSONValue]
    ) async -> Bool

    @State private var selectedSurveyID: String?
    @State private var page = 0
    @State private var pageSize = 20

    private var pagedSurveys: [LocalAgentHostRequirementSurvey] {
        surveys.agentPage(index: page, size: pageSize)
    }

    var body: some View {
        if let survey = surveys.first(where: { $0.id == selectedSurveyID }) {
            RequirementSurveyDetailView(
                survey: survey,
                isSubmitting: submittingSurveyIDs.contains(survey.id),
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
                colors: [Color(nsColor: .windowBackgroundColor), AppPalette.ai.opacity(0.035)],
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
                            Text("本项目的 Local Agent Task 还没有发起调研。")
                        }
                        .frame(maxWidth: .infinity, minHeight: 320)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18))
                    } else {
                        let open = pagedSurveys.filter { $0.status == .open }
                        let resolved = pagedSurveys.filter { $0.status == .resolved }
                        if !open.isEmpty {
                            sectionTitle("待填写", count: open.count, color: AppPalette.ai)
                            surveyGrid(open)
                        }
                        if !resolved.isEmpty {
                            sectionTitle("已恢复任务", count: resolved.count, color: .green)
                                .padding(.top, open.isEmpty ? 0 : 8)
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
        let open = surveys.filter { $0.status == .open }.count
        let resolved = surveys.filter { $0.status == .resolved }.count
        return VStack(alignment: .leading, spacing: 18) {
            HStack(alignment: .top, spacing: 16) {
                if showsNavigationBackButton {
                    Button { dismiss() } label: {
                        Label("返回项目列表", systemImage: "chevron.left")
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                }
                ZStack {
                    RoundedRectangle(cornerRadius: 14)
                        .fill(LinearGradient(
                            colors: [AppPalette.ai, .indigo],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        ))
                    Image(systemName: "text.badge.checkmark")
                        .font(.system(size: 23, weight: .semibold))
                        .foregroundStyle(.white)
                }
                .frame(width: 54, height: 54)
                VStack(alignment: .leading, spacing: 6) {
                    Text(heading).appFont(.title2.weight(.bold))
                    Text(explanation)
                        .appFont(.body)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer()
            }
            HStack(spacing: 10) {
                metric(surveys.count, "全部调研", color: .blue)
                metric(open, "待你填写", color: AppPalette.ai)
                metric(resolved, "已恢复任务", color: .green)
            }
        }
        .padding(22)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18))
        .overlay { RoundedRectangle(cornerRadius: 18).stroke(AppPalette.ai.opacity(0.13)) }
    }

    private func metric(_ value: Int, _ title: String, color: Color) -> some View {
        HStack(spacing: 9) {
            Circle().fill(color).frame(width: 8, height: 8)
            VStack(alignment: .leading, spacing: 1) {
                Text("\(value)").appFont(.headline.weight(.bold)).monospacedDigit()
                Text(title).appFont(.caption2).foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(color.opacity(0.07), in: RoundedRectangle(cornerRadius: 10))
    }

    private func sectionTitle(_ title: String, count: Int, color: Color) -> some View {
        HStack(spacing: 8) {
            Image(systemName: color == .green ? "checkmark.seal.fill" : "square.and.pencil")
                .foregroundStyle(color)
            Text(title).appFont(.headline.weight(.semibold))
            Text("\(count)")
                .appFont(.caption2)
                .foregroundStyle(color)
                .padding(.horizontal, 7)
                .padding(.vertical, 2)
                .background(color.opacity(0.1), in: Capsule())
            Spacer()
        }
    }

    private func surveyGrid(_ items: [LocalAgentHostRequirementSurvey]) -> some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 410, maximum: 680), spacing: 14)],
            alignment: .leading,
            spacing: 14
        ) {
            ForEach(items) { survey in
                Button { selectedSurveyID = survey.id } label: {
                    RequirementSurveyRow(survey: survey)
                }
                .buttonStyle(.plain)
            }
        }
    }
}
