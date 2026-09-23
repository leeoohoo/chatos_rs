import ChatOSCore
import SwiftUI

struct RequirementSurveyRow: View {
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

struct RequirementSurveyDetailView: View {
    let survey: LocalAgentRequirementSurvey
    let creatorName: String
    let projectName: String?
    let isSubmitting: Bool
    let onBack: () -> Void
    let onSubmit: (
        LocalAgentRequirementSurvey,
        [String: [String]],
        String
    ) async -> Bool
    @State private var selectedTab: RequirementSurveyDetailTab = .questionnaire

    var body: some View {
        ZStack {
            Color(nsColor: .windowBackgroundColor)
                .ignoresSafeArea()
            LinearGradient(
                colors: [
                    AppPalette.ai.opacity(0.075),
                    AppPalette.ai.opacity(0.025),
                ],
                startPoint: .top,
                endPoint: .bottom
            )
            .ignoresSafeArea()

            VStack(spacing: 0) {
                ScrollView {
                    RequirementSurveyCard(
                        survey: survey,
                        creatorName: creatorName,
                        projectName: projectName,
                        isSubmitting: isSubmitting,
                        selectedTab: $selectedTab,
                        onSubmit: onSubmit
                    )
                    .padding(.horizontal, 32)
                    .padding(.vertical, 30)
                    .frame(maxWidth: 820)
                    .frame(maxWidth: .infinity)
                }
                .safeAreaInset(edge: .top, spacing: 0) {
                    HStack {
                        Button(action: onBack) {
                            Label("返回调研列表", systemImage: "chevron.left")
                        }
                        .buttonStyle(.plain)
                        .foregroundStyle(.secondary)
                        .contentShape(Rectangle())
                        Spacer()
                    }
                    .appFont(.subheadline.weight(.medium))
                    .padding(.horizontal, 32)
                    .padding(.vertical, 14)
                    .frame(maxWidth: 820)
                    .frame(maxWidth: .infinity)
                    .background(.ultraThinMaterial)
                    .overlay(alignment: .bottom) {
                        Divider().opacity(0.45)
                    }
                }
            }
        }
    }
}

private struct RequirementSurveyCard: View {
    let survey: LocalAgentRequirementSurvey
    let creatorName: String
    let projectName: String?
    let isSubmitting: Bool
    @Binding var selectedTab: RequirementSurveyDetailTab
    let onSubmit: (
        LocalAgentRequirementSurvey,
        [String: [String]],
        String
    ) async -> Bool

    @State private var selections: [String: [String]]
    @State private var notes: String

    init(
        survey: LocalAgentRequirementSurvey,
        creatorName: String,
        projectName: String?,
        isSubmitting: Bool,
        selectedTab: Binding<RequirementSurveyDetailTab>,
        onSubmit: @escaping (
            LocalAgentRequirementSurvey,
            [String: [String]],
            String
        ) async -> Bool
    ) {
        self.survey = survey
        self.creatorName = creatorName
        self.projectName = projectName
        self.isSubmitting = isSubmitting
        _selectedTab = selectedTab
        self.onSubmit = onSubmit
        let submittedAnswers = Dictionary(
            uniqueKeysWithValues: (survey.submission?.answers ?? []).map {
                ($0.questionID, $0.selectedOptionIDs)
            }
        )
        _selections = State(initialValue: Dictionary(
            uniqueKeysWithValues: survey.draft.questions.map { question in
                if let submitted = submittedAnswers[question.id] {
                    return (question.id, submitted)
                }
                if survey.status == .pending,
                   question.kind == .ranking,
                   question.isRequired {
                    return (question.id, question.options.map(\.id))
                }
                return (question.id, [])
            }
        ))
        _notes = State(initialValue: survey.submission?.notes ?? "")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            surveyHeader
            contentTabs
                .padding(.top, 24)
                .padding(.bottom, 40)

            if selectedTab == .questionnaire {
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
    }

    private var surveyHeader: some View {
        VStack(alignment: .leading, spacing: 13) {
            HStack(alignment: .center, spacing: 10) {
                Label("需求调研", systemImage: "list.clipboard")
                    .appFont(.caption.weight(.semibold))
                    .foregroundStyle(AppPalette.ai)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 5)
                    .background(AppPalette.ai.opacity(0.1), in: Capsule())

                Text(statusTitle)
                    .appFont(.caption.weight(.medium))
                    .foregroundStyle(statusColor)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 5)
                    .background(statusColor.opacity(0.1), in: Capsule())
            }

            Text(survey.draft.title)
                .appFont(.title2.weight(.bold))
                .fixedSize(horizontal: false, vertical: true)

            Text(survey.draft.purpose)
                .appFont(.body)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            HStack(spacing: 12) {
                Label("由 \(creatorName) 发起", systemImage: "person")
                if let projectName {
                    Label(projectName, systemImage: "folder")
                }
                if survey.status == .pending {
                    Text("已完成 \(answeredQuestionCount)/\(survey.draft.questions.count)")
                        .monospacedDigit()
                }
            }
            .appFont(.caption)
            .foregroundStyle(.tertiary)
        }
    }

    private var contentTabs: some View {
        HStack(spacing: 6) {
            ForEach(RequirementSurveyDetailTab.allCases) { tab in
                Button {
                    selectedTabBinding(tab)
                } label: {
                    Text(tab.rawValue)
                        .appFont(.subheadline.weight(.medium))
                        .foregroundStyle(selectedTab == tab ? .white : .secondary)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 7)
                        .background(
                            selectedTab == tab ? AppPalette.ai : Color.clear,
                            in: Capsule()
                        )
                }
                .buttonStyle(.plain)
            }
        }
        .padding(4)
        .background(AppPalette.inputSurface.opacity(0.7), in: Capsule())
        .overlay {
            Capsule().stroke(AppPalette.border.opacity(0.65), lineWidth: 1)
        }
        .fixedSize()
    }

    private func selectedTabBinding(_ tab: RequirementSurveyDetailTab) {
        withAnimation(.easeOut(duration: 0.16)) {
            selectedTab = tab
        }
    }

    private var questionnaireContent: some View {
        VStack(alignment: .leading, spacing: 42) {
            ForEach(Array(survey.draft.questions.enumerated()), id: \.element.id) { index, question in
                VStack(alignment: .leading, spacing: 13) {
                    HStack(alignment: .firstTextBaseline, spacing: 5) {
                        Text("\(index + 1).")
                        Text(question.prompt)
                            .fontWeight(.medium)
                        if question.isRequired {
                            Text("*")
                                .appFont(.caption2.weight(.bold))
                                .foregroundStyle(.secondary)
                                .frame(width: 16, height: 16)
                                .background(.primary.opacity(0.08), in: Circle())
                        }
                    }
                    .appFont(.title3)

                    if question.kind == .multipleChoice {
                        Text("请选择所有符合的选项")
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                    } else if question.kind == .ranking {
                        Text("请按优先级排序，1 表示最高优先级")
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        if question.kind == .ranking {
                            ForEach(Array(rankedOptions(for: question).enumerated()), id: \.element.id) { optionIndex, option in
                                rankingOptionRow(
                                    option,
                                    rank: optionIndex,
                                    question: question
                                )
                            }
                        } else {
                            ForEach(Array(question.options.enumerated()), id: \.element.id) { optionIndex, option in
                                optionButton(option, optionIndex: optionIndex, question: question)
                            }
                        }
                    }
                    .padding(.top, 1)
                }
            }

            notesContent

            if survey.status == .pending {
                HStack(alignment: .center, spacing: 16) {
                    if !canSubmit {
                        Label("请完成所有必填题", systemImage: "asterisk")
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 0)
                    Button {
                        Task { _ = await onSubmit(survey, selections, notes) }
                    } label: {
                        if isSubmitting {
                            ProgressView().controlSize(.small)
                        } else {
                            Label("提交调研", systemImage: "arrow.right")
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .tint(AppPalette.ai)
                    .disabled(isSubmitting || !canSubmit)
                }
                .padding(.top, 4)
            }
        }
    }

    private var notesContent: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("补充说明")
                .appFont(.title3.weight(.medium))
            Text("如果选项没有覆盖完整情况，可以在这里统一补充。")
                .appFont(.caption)
                .foregroundStyle(.secondary)
            if survey.status == .pending {
                TextEditor(text: $notes)
                    .scrollContentBackground(.hidden)
                    .appFont(.body)
                    .frame(minHeight: 112)
                    .padding(10)
                    .background(AppPalette.inputSurface.opacity(0.92), in: RoundedRectangle(cornerRadius: 9))
                    .overlay {
                        RoundedRectangle(cornerRadius: 9)
                            .stroke(AppPalette.border.opacity(0.9), lineWidth: 1)
                    }
                    .shadow(color: .black.opacity(0.035), radius: 2, y: 1)
            } else {
                Text(notes.isEmpty ? "无" : notes)
                    .appFont(.body)
                    .foregroundStyle(notes.isEmpty ? .secondary : .primary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(10)
                    .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 10))
            }
        }
    }

    private var answeredQuestionCount: Int {
        survey.draft.questions.filter { !(selections[$0.id] ?? []).isEmpty }.count
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

    private var statusColor: Color {
        if survey.status == .pending { return AppPalette.ai }
        return survey.resolution == nil ? .orange : .green
    }

    private func resolutionView(
        _ resolution: LocalAgentRequirementSurveyResolution
    ) -> some View {
        VStack(alignment: .leading, spacing: 30) {
            Label("处理结果", systemImage: "checkmark.seal.fill")
                .appFont(.title3.weight(.semibold))
                .foregroundStyle(AppPalette.ai)

            VStack(alignment: .leading, spacing: 8) {
                Text("结论摘要").appFont(.headline).fontWeight(.semibold)
                Text(resolution.summary)
                    .appFont(.body)
                    .fixedSize(horizontal: false, vertical: true)
            }

            VStack(alignment: .leading, spacing: 10) {
                Text("解决方案").appFont(.headline).fontWeight(.semibold)
                MarkdownDocumentView(
                    markdown: resolution.solutionMarkdown,
                    allowsTextSelection: true
                )
            }

            VStack(alignment: .leading, spacing: 11) {
                Text("执行计划").appFont(.headline).fontWeight(.semibold)
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
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AppPalette.inputSurface.opacity(0.85), in: RoundedRectangle(cornerRadius: 10))
                    .overlay {
                        RoundedRectangle(cornerRadius: 10)
                            .stroke(AppPalette.border.opacity(0.65), lineWidth: 1)
                    }
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
        optionIndex: Int,
        question: LocalAgentRequirementSurveyQuestion
    ) -> some View {
        let selected = selections[question.id, default: []].contains(option.id)
        return Button {
            guard survey.status == .pending else { return }
            if question.kind == .singleChoice {
                selections[question.id] = [option.id]
            } else if selected {
                selections[question.id, default: []].removeAll { $0 == option.id }
            } else {
                let updated = Set(selections[question.id, default: []] + [option.id])
                selections[question.id] = question.options.map(\.id).filter(updated.contains)
            }
        } label: {
            if question.kind == .multipleChoice {
                HStack(spacing: 10) {
                    ZStack {
                        RoundedRectangle(cornerRadius: 4)
                            .fill(selected ? AppPalette.ai : AppPalette.inputSurface)
                        RoundedRectangle(cornerRadius: 4)
                            .stroke(selected ? AppPalette.ai : AppPalette.border, lineWidth: 1)
                        if selected {
                            Image(systemName: "checkmark")
                                .font(.system(size: 10, weight: .bold))
                                .foregroundStyle(.white)
                        }
                    }
                    .frame(width: 19, height: 19)

                    Text(option.label)
                        .appFont(.body)
                        .foregroundStyle(.primary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(.vertical, 3)
                .contentShape(Rectangle())
            } else {
                HStack(spacing: 10) {
                    Text(optionLetter(optionIndex))
                        .appFont(.caption2.weight(.bold))
                        .foregroundStyle(selected ? .white : .secondary)
                        .frame(width: 19, height: 19)
                        .background(
                            selected ? AppPalette.ai.opacity(0.72) : Color.primary.opacity(0.08),
                            in: RoundedRectangle(cornerRadius: 4)
                        )
                    Text(option.label)
                        .appFont(.body)
                        .foregroundStyle(.primary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
                .background(AppPalette.inputSurface.opacity(0.92), in: RoundedRectangle(cornerRadius: 8))
                .overlay {
                    RoundedRectangle(cornerRadius: 8)
                        .stroke(selected ? AppPalette.ai.opacity(0.72) : AppPalette.border, lineWidth: selected ? 2 : 1)
                }
                .shadow(color: .black.opacity(0.045), radius: 2, y: 1)
            }
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity, alignment: .leading)
        .disabled(survey.status == .submitted)
    }

    private func rankedOptions(
        for question: LocalAgentRequirementSurveyQuestion
    ) -> [LocalAgentRequirementSurveyOption] {
        let optionsByID = Dictionary(uniqueKeysWithValues: question.options.map { ($0.id, $0) })
        let rankedIDs = selections[question.id] ?? []
        let ranked = rankedIDs.compactMap { optionsByID[$0] }
        let rankedIDSet = Set(rankedIDs)
        return ranked + question.options.filter { !rankedIDSet.contains($0.id) }
    }

    private func rankingOptionRow(
        _ option: LocalAgentRequirementSurveyOption,
        rank: Int,
        question: LocalAgentRequirementSurveyQuestion
    ) -> some View {
        HStack(spacing: 10) {
            Text("\(rank + 1)")
                .appFont(.caption.weight(.bold))
                .foregroundStyle(.white)
                .frame(width: 21, height: 21)
                .background(AppPalette.ai.opacity(0.72), in: RoundedRectangle(cornerRadius: 4))

            Text(option.label)
                .appFont(.body)
                .fixedSize(horizontal: false, vertical: true)

            Spacer(minLength: 10)

            HStack(spacing: 2) {
                rankingMoveButton(
                    systemImage: "chevron.up",
                    help: "上移",
                    disabled: rank == 0,
                    action: { moveRankingOption(at: rank, offset: -1, question: question) }
                )
                rankingMoveButton(
                    systemImage: "chevron.down",
                    help: "下移",
                    disabled: rank == question.options.count - 1,
                    action: { moveRankingOption(at: rank, offset: 1, question: question) }
                )
            }
        }
        .padding(.leading, 10)
        .padding(.trailing, 6)
        .padding(.vertical, 7)
        .background(AppPalette.inputSurface.opacity(0.92), in: RoundedRectangle(cornerRadius: 8))
        .overlay {
            RoundedRectangle(cornerRadius: 8)
                .stroke(AppPalette.border, lineWidth: 1)
        }
        .shadow(color: .black.opacity(0.04), radius: 2, y: 1)
    }

    private func rankingMoveButton(
        systemImage: String,
        help: String,
        disabled: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .font(.system(size: 10, weight: .semibold))
                .frame(width: 24, height: 24)
        }
        .buttonStyle(.plain)
        .foregroundStyle(disabled ? Color.secondary.opacity(0.35) : AppPalette.ai)
        .contentShape(Rectangle())
        .disabled(disabled || survey.status == .submitted)
        .help(help)
    }

    private func moveRankingOption(
        at index: Int,
        offset: Int,
        question: LocalAgentRequirementSurveyQuestion
    ) {
        guard survey.status == .pending else { return }
        var ordered = rankedOptions(for: question).map(\.id)
        let destination = index + offset
        guard ordered.indices.contains(index), ordered.indices.contains(destination) else { return }
        ordered.swapAt(index, destination)
        selections[question.id] = ordered
    }

    private func optionLetter(_ index: Int) -> String {
        guard index >= 0, index < 26 else { return "\(index + 1)" }
        return String(UnicodeScalar(65 + index)!)
    }
}
