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
        [String: Set<String>],
        String
    ) async -> Bool
    @State private var selectedSurveyID: String?

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
                        let pending = surveys.filter { $0.status == .pending }
                        let awaitingResolution = surveys.filter {
                            $0.status == .submitted && $0.resolution == nil
                        }
                        let resolved = surveys.filter { $0.resolution != nil }
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

private struct RequirementSurveyRow: View {
    let survey: LocalAgentRequirementSurvey
    let creatorName: String
    let projectName: String?
    @State private var isHovering = false

    var body: some View {
        HStack(alignment: .top, spacing: 14) {
            Image(systemName: statusIcon)
                .font(.title3)
                .foregroundStyle(statusColor)
                .frame(width: 34, height: 34)
                .background(statusColor.opacity(0.1), in: RoundedRectangle(cornerRadius: 9))

            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .firstTextBaseline, spacing: 10) {
                    Text(survey.draft.title)
                        .appFont(.headline)
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    Text(statusTitle)
                        .appFont(.caption)
                        .foregroundStyle(statusColor)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .background(statusColor.opacity(0.1), in: Capsule())
                }

                Text(survey.draft.purpose)
                    .appFont(.body)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)

                HStack(spacing: 12) {
                    Label("\(survey.draft.questions.count) 个问题", systemImage: "list.number")
                    Label("由 \(creatorName) 发起", systemImage: "person")
                    Label(createdAt, systemImage: "clock")
                    if let projectName {
                        Label(projectName, systemImage: "folder.fill")
                    }
                }
                .appFont(.caption2)
                .foregroundStyle(.tertiary)

                HStack(spacing: 5) {
                    Text("查看完整调研")
                    Image(systemName: "arrow.right")
                }
                .appFont(.caption.weight(.semibold))
                .foregroundStyle(statusColor)
            }

            Image(systemName: "chevron.right")
                .appFont(.caption)
                .foregroundStyle(.tertiary)
                .padding(.top, 10)
        }
        .padding(16)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(statusColor.opacity(isHovering ? 0.35 : 0.14), lineWidth: 1)
        }
        .overlay(alignment: .leading) {
            Capsule()
                .fill(statusColor)
                .frame(width: 4)
                .padding(.vertical, 14)
                .padding(.leading, 2)
        }
        .shadow(
            color: .black.opacity(isHovering ? 0.07 : 0.035),
            radius: isHovering ? 10 : 5,
            y: isHovering ? 4 : 2
        )
        .scaleEffect(isHovering ? 1.004 : 1)
        .animation(.easeOut(duration: 0.16), value: isHovering)
        .onHover { isHovering = $0 }
        .contentShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private var statusTitle: String {
        if survey.status == .pending { return "待填写" }
        return survey.resolution == nil ? "形成方案中" : "已形成方案"
    }

    private var statusIcon: String {
        if survey.status == .pending { return "square.and.pencil" }
        return survey.resolution == nil ? "hourglass" : "checkmark.seal.fill"
    }

    private var statusColor: Color {
        if survey.status == .pending { return AppPalette.ai }
        return survey.resolution == nil ? .orange : .green
    }

    private var createdAt: String {
        Date(timeIntervalSince1970: TimeInterval(survey.createdAtUnixMs) / 1_000)
            .formatted(date: .abbreviated, time: .shortened)
    }
}

private enum RequirementSurveyDetailTab: String, CaseIterable, Identifiable {
    case questionnaire = "调研问卷"
    case resolution = "方案与执行"

    var id: Self { self }
}

private struct RequirementSurveyDetailView: View {
    let survey: LocalAgentRequirementSurvey
    let creatorName: String
    let projectName: String?
    let isSubmitting: Bool
    let onBack: () -> Void
    let onSubmit: (
        LocalAgentRequirementSurvey,
        [String: Set<String>],
        String
    ) async -> Bool
    @State private var selectedTab: RequirementSurveyDetailTab = .questionnaire

    var body: some View {
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

            VStack(spacing: 16) {
                VStack(spacing: 16) {
                    HStack {
                        Button(action: onBack) {
                            Label("返回调研列表", systemImage: "chevron.left")
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        Spacer()
                    }
                    RequirementSurveyStageBar(survey: survey)

                    HStack {
                        Picker("调研内容", selection: $selectedTab) {
                            ForEach(RequirementSurveyDetailTab.allCases) { tab in
                                Text(tab.rawValue).tag(tab)
                            }
                        }
                        .pickerStyle(.segmented)
                        .labelsHidden()
                        .frame(maxWidth: 460)
                        Spacer()
                    }
                }
                .padding(.horizontal, 24)
                .padding(.top, 24)
                .frame(maxWidth: 1560)
                .frame(maxWidth: .infinity)

                ScrollView {
                    RequirementSurveyCard(
                        survey: survey,
                        creatorName: creatorName,
                        projectName: projectName,
                        isSubmitting: isSubmitting,
                        selectedTab: selectedTab,
                        onSubmit: onSubmit
                    )
                    .padding(.horizontal, 24)
                    .padding(.bottom, 24)
                    .frame(maxWidth: 1560)
                    .frame(maxWidth: .infinity)
                }
                .id(selectedTab)
            }
        }
    }
}

private struct RequirementSurveyStageBar: View {
    let survey: LocalAgentRequirementSurvey

    var body: some View {
        HStack(spacing: 0) {
            stage(number: 1, title: "调研已发起", state: .complete)
            connector(completed: survey.status != .pending)
            stage(
                number: 2,
                title: survey.status == .pending ? "等待填写" : "答案已提交",
                state: survey.status == .pending ? .active : .complete
            )
            connector(completed: survey.resolution != nil)
            stage(
                number: 3,
                title: survey.resolution == nil ? "形成方案" : "方案已完成",
                state: survey.resolution == nil && survey.status != .pending ? .active
                    : (survey.resolution == nil ? .upcoming : .complete)
            )
        }
        .padding(.horizontal, 22)
        .padding(.vertical, 16)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(AppPalette.ai.opacity(0.12), lineWidth: 1)
        }
        .shadow(color: .black.opacity(0.03), radius: 8, y: 3)
    }

    private enum StageState {
        case complete
        case active
        case upcoming
    }

    private func stage(number: Int, title: String, state: StageState) -> some View {
        let color: Color = state == .upcoming ? .secondary : AppPalette.ai
        return HStack(spacing: 8) {
            ZStack {
                Circle()
                    .fill(state == .complete ? AppPalette.ai : color.opacity(0.11))
                if state == .complete {
                    Image(systemName: "checkmark")
                        .font(.caption.weight(.bold))
                        .foregroundStyle(.white)
                } else {
                    Text("\(number)")
                        .appFont(.caption.weight(.bold))
                        .foregroundStyle(color)
                }
            }
            .frame(width: 28, height: 28)

            Text(title)
                .appFont(.caption.weight(state == .active ? .semibold : .medium))
                .foregroundStyle(state == .upcoming ? .secondary : .primary)
                .fixedSize()
        }
    }

    private func connector(completed: Bool) -> some View {
        Capsule()
            .fill(completed ? AppPalette.ai : AppPalette.border)
            .frame(maxWidth: .infinity)
            .frame(height: 2)
            .padding(.horizontal, 10)
    }
}

private struct RequirementSurveyCard: View {
    let survey: LocalAgentRequirementSurvey
    let creatorName: String
    let projectName: String?
    let isSubmitting: Bool
    let selectedTab: RequirementSurveyDetailTab
    let onSubmit: (
        LocalAgentRequirementSurvey,
        [String: Set<String>],
        String
    ) async -> Bool

    @State private var selections: [String: Set<String>]
    @State private var notes: String

    init(
        survey: LocalAgentRequirementSurvey,
        creatorName: String,
        projectName: String?,
        isSubmitting: Bool,
        selectedTab: RequirementSurveyDetailTab,
        onSubmit: @escaping (
            LocalAgentRequirementSurvey,
            [String: Set<String>],
            String
        ) async -> Bool
    ) {
        self.survey = survey
        self.creatorName = creatorName
        self.projectName = projectName
        self.isSubmitting = isSubmitting
        self.selectedTab = selectedTab
        self.onSubmit = onSubmit
        _selections = State(initialValue: Dictionary(
            uniqueKeysWithValues: (survey.submission?.answers ?? []).map {
                ($0.questionID, Set($0.selectedOptionIDs))
            }
        ))
        _notes = State(initialValue: survey.submission?.notes ?? "")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(alignment: .top, spacing: 12) {
                ZStack {
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .fill(AppPalette.ai.opacity(0.1))
                    Image(systemName: "doc.questionmark.fill")
                        .font(.system(size: 20, weight: .semibold))
                        .foregroundStyle(AppPalette.ai)
                }
                .frame(width: 46, height: 46)

                VStack(alignment: .leading, spacing: 5) {
                    Text(survey.draft.title)
                        .appFont(.title3.weight(.bold))
                    Text(survey.draft.purpose)
                        .appFont(.body)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                    Text("由 \(creatorName) 发起")
                        .appFont(.caption2)
                        .foregroundStyle(.tertiary)
                    if let projectName {
                        Label(projectName, systemImage: "folder.fill")
                            .appFont(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
                Spacer()
                Text(statusTitle)
                    .appFont(.caption)
                    .foregroundStyle(survey.status == .pending ? AppPalette.ai : .secondary)
                    .padding(.horizontal, 9)
                    .padding(.vertical, 5)
                    .background(
                        (survey.status == .pending ? AppPalette.aiSoft : AppPalette.inputSurface),
                        in: Capsule()
                    )
            }

            if selectedTab == .questionnaire {
                if survey.status == .pending {
                    let answered = survey.draft.questions.filter {
                        !(selections[$0.id] ?? []).isEmpty
                    }.count
                    VStack(alignment: .leading, spacing: 7) {
                        HStack {
                            Text("填写进度")
                                .appFont(.caption.weight(.semibold))
                            Spacer()
                            Text("\(answered) / \(survey.draft.questions.count)")
                                .appFont(.caption.weight(.semibold))
                                .monospacedDigit()
                                .foregroundStyle(AppPalette.ai)
                        }
                        ProgressView(
                            value: Double(answered),
                            total: Double(max(1, survey.draft.questions.count))
                        )
                        .tint(AppPalette.ai)
                    }
                    .padding(12)
                    .background(AppPalette.ai.opacity(0.06), in: RoundedRectangle(cornerRadius: 10))
                }

                questionnaireContent
            } else if let resolution = survey.resolution {
                resolutionView(resolution)
            } else if survey.status == .submitted {
                Label("答案已通知项目经理，正在形成解决方案和执行计划", systemImage: "hourglass")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AppPalette.aiSoft.opacity(0.6), in: RoundedRectangle(cornerRadius: 10))
            } else {
                Label("提交调研后，解决方案和执行计划会显示在这里", systemImage: "doc.text")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 10))
            }
        }
        .padding(22)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .stroke(AppPalette.ai.opacity(0.13), lineWidth: 1)
        }
        .shadow(color: .black.opacity(0.04), radius: 12, y: 5)
    }

    private var questionnaireContent: some View {
        Group {
            ForEach(Array(survey.draft.questions.enumerated()), id: \.element.id) { index, question in
                VStack(alignment: .leading, spacing: 9) {
                    HStack(alignment: .top, spacing: 9) {
                        Text("\(index + 1)")
                            .appFont(.caption.weight(.bold))
                            .foregroundStyle(AppPalette.ai)
                            .frame(width: 24, height: 24)
                            .background(AppPalette.ai.opacity(0.1), in: Circle())
                        Text(question.prompt)
                            .appFont(.subheadline)
                            .fontWeight(.medium)
                            .padding(.top, 2)
                        if question.isRequired {
                            Text("*")
                                .foregroundStyle(.red)
                                .padding(.top, 2)
                        }
                        Spacer(minLength: 8)
                        if question.kind == .multipleChoice {
                            Text("可多选")
                                .appFont(.caption2)
                                .foregroundStyle(AppPalette.ai)
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(AppPalette.ai.opacity(0.08), in: Capsule())
                        }
                    }
                    SurveyOptionFlowLayout(minItemWidth: 230, spacing: 9) {
                        ForEach(question.options) { option in
                            optionButton(option, question: question)
                        }
                    }
                }
                .padding(14)
                .background(
                    AppPalette.surfaceSubtle.opacity(0.72),
                    in: RoundedRectangle(cornerRadius: 12, style: .continuous)
                )
                .overlay {
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .stroke(AppPalette.border.opacity(0.7), lineWidth: 1)
                }
            }

            notesContent

            if survey.status == .pending {
                HStack {
                    Spacer()
                    Button {
                        Task { _ = await onSubmit(survey, selections, notes) }
                    } label: {
                        if isSubmitting {
                            ProgressView().controlSize(.small)
                        } else {
                            Text("提交调研")
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(isSubmitting || !canSubmit)
                }
            }
        }
    }

    private var notesContent: some View {
        VStack(alignment: .leading, spacing: 7) {
            Label("备注", systemImage: "text.alignleft")
                .appFont(.subheadline.weight(.semibold))
                .foregroundStyle(AppPalette.ai)
            Text("如果选项没有覆盖完整情况，可在这里统一补充。")
                .appFont(.caption)
                .foregroundStyle(.secondary)
            if survey.status == .pending {
                TextEditor(text: $notes)
                    .scrollContentBackground(.hidden)
                    .appFont(.body)
                    .frame(minHeight: 90)
                    .padding(8)
                    .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 10))
                    .overlay {
                        RoundedRectangle(cornerRadius: 10)
                            .stroke(AppPalette.border, lineWidth: 1)
                    }
            } else {
                Text(notes.isEmpty ? "无" : notes)
                    .appFont(.body)
                    .foregroundStyle(notes.isEmpty ? .secondary : .primary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(10)
                    .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 10))
            }
        }
        .padding(14)
        .background(AppPalette.ai.opacity(0.045), in: RoundedRectangle(cornerRadius: 12))
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .stroke(AppPalette.ai.opacity(0.13), lineWidth: 1)
        }
    }

    private var canSubmit: Bool {
        survey.draft.questions.allSatisfy { question in
            !question.isRequired || !(selections[question.id] ?? []).isEmpty
        }
    }

    private var statusTitle: String {
        if survey.status == .pending { return "待填写" }
        return survey.resolution == nil ? "形成方案中" : "已形成方案"
    }

    private func resolutionView(
        _ resolution: LocalAgentRequirementSurveyResolution
    ) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Divider()
            Label("处理结果", systemImage: "checkmark.seal.fill")
                .appFont(.headline)
                .foregroundStyle(AppPalette.ai)

            VStack(alignment: .leading, spacing: 5) {
                Text("结论摘要").appFont(.subheadline).fontWeight(.semibold)
                Text(resolution.summary)
                    .appFont(.body)
                    .fixedSize(horizontal: false, vertical: true)
            }

            VStack(alignment: .leading, spacing: 7) {
                Text("解决方案").appFont(.subheadline).fontWeight(.semibold)
                MarkdownDocumentView(
                    markdown: resolution.solutionMarkdown,
                    allowsTextSelection: true
                )
            }

            VStack(alignment: .leading, spacing: 9) {
                Text("执行计划").appFont(.subheadline).fontWeight(.semibold)
                ForEach(Array(resolution.executionSteps.enumerated()), id: \.element.id) { index, step in
                    VStack(alignment: .leading, spacing: 5) {
                        Text("\(index + 1). \(step.title)")
                            .appFont(.subheadline)
                            .fontWeight(.medium)
                        Text(step.detail)
                            .appFont(.body)
                            .foregroundStyle(.secondary)
                        if !step.owner.isEmpty {
                            Text("负责人：\(step.owner)").appFont(.caption)
                        }
                        if !step.deliverable.isEmpty {
                            Text("交付物：\(step.deliverable)").appFont(.caption)
                        }
                        if !step.acceptanceCriteria.isEmpty {
                            Text("验收标准：\(step.acceptanceCriteria)").appFont(.caption)
                        }
                    }
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 10))
                }
            }

            if !resolution.risksAndOpenQuestions.isEmpty {
                VStack(alignment: .leading, spacing: 7) {
                    Text("风险与待确认事项").appFont(.subheadline).fontWeight(.semibold)
                    MarkdownDocumentView(
                        markdown: resolution.risksAndOpenQuestions,
                        allowsTextSelection: true
                    )
                }
            }
            if !resolution.relatedMaterials.isEmpty {
                VStack(alignment: .leading, spacing: 7) {
                    Text("相关资料").appFont(.subheadline).fontWeight(.semibold)
                    MarkdownDocumentView(
                        markdown: resolution.relatedMaterials,
                        allowsTextSelection: true
                    )
                }
            }
        }
    }

    private func optionButton(
        _ option: LocalAgentRequirementSurveyOption,
        question: LocalAgentRequirementSurveyQuestion
    ) -> some View {
        let selected = selections[question.id, default: []].contains(option.id)
        return Button {
            guard survey.status == .pending else { return }
            if question.kind == .singleChoice {
                selections[question.id] = [option.id]
            } else if selected {
                selections[question.id, default: []].remove(option.id)
            } else {
                selections[question.id, default: []].insert(option.id)
            }
        } label: {
            HStack(spacing: 9) {
                Image(systemName: question.kind == .multipleChoice
                      ? (selected ? "checkmark.square.fill" : "square")
                      : (selected ? "circle.inset.filled" : "circle"))
                    .foregroundStyle(selected ? AppPalette.ai : .secondary)
                Text(option.label)
                    .appFont(.subheadline)
                    .foregroundStyle(.primary)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
            }
            .padding(10)
            .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
            .background(
                selected ? AppPalette.ai.opacity(0.08) : AppPalette.inputSurface,
                in: RoundedRectangle(cornerRadius: 10)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 10)
                    .stroke(selected ? AppPalette.ai.opacity(0.5) : AppPalette.border, lineWidth: 1)
            }
        }
        .buttonStyle(.plain)
        .disabled(survey.status == .submitted)
    }
}
