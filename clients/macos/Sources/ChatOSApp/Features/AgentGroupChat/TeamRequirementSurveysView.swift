import ChatOSCore
import SwiftUI

struct ProjectRequirementSurveysView: View {
    let surveys: [LocalAgentRequirementSurvey]
    let submittingSurveyIDs: Set<String>
    let creatorNamesByID: [String: String]
    var projectNamesByID: [String: String] = [:]
    var heading = "需求调研"
    var explanation = "项目经理可在新需求、重大变更或任何信息不足的节点发起调研。选择答案后，页面末尾可统一补充备注。"
    let onSubmit: (
        LocalAgentRequirementSurvey,
        [String: Set<String>],
        String
    ) async -> Bool

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 16) {
                VStack(alignment: .leading, spacing: 5) {
                    Text(heading)
                        .appFont(.title2)
                        .fontWeight(.semibold)
                    Text(explanation)
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }

                if surveys.isEmpty {
                    ContentUnavailableView {
                        Label("暂无需求调研", systemImage: "list.clipboard")
                    } description: {
                        Text("项目经理需要确认目标、范围或方案时，会在这里创建调研单。")
                    }
                    .frame(maxWidth: .infinity, minHeight: 320)
                } else {
                    let pending = surveys.filter { $0.status == .pending }
                    let awaitingResolution = surveys.filter {
                        $0.status == .submitted && $0.resolution == nil
                    }
                    let resolved = surveys.filter { $0.resolution != nil }
                    if !pending.isEmpty {
                        sectionTitle("待填写", count: pending.count)
                        ForEach(pending) { survey in
                            RequirementSurveyCard(
                                survey: survey,
                                creatorName: creatorNamesByID[survey.creatorAgentID] ?? "项目经理",
                                projectName: projectNamesByID[survey.projectID],
                                isSubmitting: submittingSurveyIDs.contains(survey.id),
                                onSubmit: onSubmit
                            )
                        }
                    }
                    if !awaitingResolution.isEmpty {
                        sectionTitle("等待项目经理形成方案", count: awaitingResolution.count)
                            .padding(.top, pending.isEmpty ? 0 : 8)
                        ForEach(awaitingResolution) { survey in
                            RequirementSurveyCard(
                                survey: survey,
                                creatorName: creatorNamesByID[survey.creatorAgentID] ?? "项目经理",
                                projectName: projectNamesByID[survey.projectID],
                                isSubmitting: false,
                                onSubmit: onSubmit
                            )
                        }
                    }
                    if !resolved.isEmpty {
                        sectionTitle("已形成方案", count: resolved.count)
                            .padding(.top, pending.isEmpty && awaitingResolution.isEmpty ? 0 : 8)
                        ForEach(resolved) { survey in
                            RequirementSurveyCard(
                                survey: survey,
                                creatorName: creatorNamesByID[survey.creatorAgentID] ?? "项目经理",
                                projectName: projectNamesByID[survey.projectID],
                                isSubmitting: false,
                                onSubmit: onSubmit
                            )
                        }
                    }
                }
            }
            .padding(20)
            .frame(maxWidth: 820, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .background(AppPalette.canvas)
    }

    private func sectionTitle(_ title: String, count: Int) -> some View {
        HStack(spacing: 7) {
            Text(title).appFont(.headline)
            Text("\(count)")
                .appFont(.caption2)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 7)
                .padding(.vertical, 2)
                .background(AppPalette.inputSurface, in: Capsule())
        }
    }
}

private struct RequirementSurveyCard: View {
    let survey: LocalAgentRequirementSurvey
    let creatorName: String
    let projectName: String?
    let isSubmitting: Bool
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
                VStack(alignment: .leading, spacing: 5) {
                    Text(survey.draft.title)
                        .appFont(.headline)
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

            ForEach(Array(survey.draft.questions.enumerated()), id: \.element.id) { index, question in
                VStack(alignment: .leading, spacing: 9) {
                    HStack(spacing: 5) {
                        Text("\(index + 1). \(question.prompt)")
                            .appFont(.subheadline)
                            .fontWeight(.medium)
                        if question.isRequired {
                            Text("*").foregroundStyle(.red)
                        }
                        if question.kind == .multipleChoice {
                            Text("可多选")
                                .appFont(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 230), spacing: 9)],
                        alignment: .leading,
                        spacing: 9
                    ) {
                        ForEach(question.options) { option in
                            optionButton(option, question: question)
                        }
                    }
                }
            }

            VStack(alignment: .leading, spacing: 7) {
                Text("备注")
                    .appFont(.subheadline)
                    .fontWeight(.medium)
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

            if let resolution = survey.resolution {
                resolutionView(resolution)
            } else if survey.status == .submitted {
                Label("答案已通知项目经理，正在形成解决方案和执行计划", systemImage: "hourglass")
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AppPalette.aiSoft.opacity(0.6), in: RoundedRectangle(cornerRadius: 10))
            }

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
        .padding(18)
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14)
                .stroke(AppPalette.border, lineWidth: 1)
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
