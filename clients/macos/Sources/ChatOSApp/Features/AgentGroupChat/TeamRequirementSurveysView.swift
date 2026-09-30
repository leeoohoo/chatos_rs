import ChatOSConnector
import SwiftUI

struct RequirementSurveyRow: View {
    let survey: LocalAgentHostRequirementSurvey
    @State private var isHovering = false

    var body: some View {
        HStack(alignment: .top, spacing: 14) {
            Image(systemName: survey.status == .open ? "square.and.pencil" : "checkmark.seal.fill")
                .font(.title3)
                .foregroundStyle(statusColor)
                .frame(width: 34, height: 34)
                .background(statusColor.opacity(0.1), in: RoundedRectangle(cornerRadius: 9))
            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .firstTextBaseline, spacing: 10) {
                    Text(survey.title).appFont(.headline).lineLimit(1)
                    Spacer(minLength: 8)
                    Text(survey.status == .open ? "待填写" : "已恢复任务")
                        .appFont(.caption)
                        .foregroundStyle(statusColor)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .background(statusColor.opacity(0.1), in: Capsule())
                }
                if let description = survey.description, !description.isEmpty {
                    Text(description).appFont(.body).foregroundStyle(.secondary).lineLimit(2)
                }
                HStack(spacing: 12) {
                    Label("\(survey.questions.count) 个问题", systemImage: "list.number")
                    Label(createdAt, systemImage: "clock")
                    if survey.sourceTaskID != nil {
                        Label("Task", systemImage: "checklist")
                    }
                }
                .appFont(.caption2)
                .foregroundStyle(.tertiary)
                Label("查看完整调研", systemImage: "arrow.right")
                    .appFont(.caption.weight(.semibold))
                    .foregroundStyle(statusColor)
            }
            Image(systemName: "chevron.right")
                .appFont(.caption)
                .foregroundStyle(.tertiary)
                .padding(.top, 10)
        }
        .padding(16)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
        .overlay { RoundedRectangle(cornerRadius: 14).stroke(statusColor.opacity(isHovering ? 0.35 : 0.14)) }
        .shadow(color: .black.opacity(isHovering ? 0.07 : 0.035), radius: isHovering ? 10 : 5, y: 3)
        .scaleEffect(isHovering ? 1.004 : 1)
        .animation(.easeOut(duration: 0.16), value: isHovering)
        .onHover { isHovering = $0 }
    }

    private var statusColor: Color { survey.status == .open ? AppPalette.ai : .green }
    private var createdAt: String {
        Date(timeIntervalSince1970: TimeInterval(survey.createdAtUnixMs) / 1_000)
            .formatted(date: .abbreviated, time: .shortened)
    }
}

struct RequirementSurveyDetailView: View {
    let survey: LocalAgentHostRequirementSurvey
    let isSubmitting: Bool
    let onBack: () -> Void
    let onSubmit: (
        LocalAgentHostRequirementSurvey,
        [String: LocalAgentJSONValue]
    ) async -> Bool

    @State private var answers: [String: LocalAgentJSONValue]

    init(
        survey: LocalAgentHostRequirementSurvey,
        isSubmitting: Bool,
        onBack: @escaping () -> Void,
        onSubmit: @escaping (
            LocalAgentHostRequirementSurvey,
            [String: LocalAgentJSONValue]
        ) async -> Bool
    ) {
        self.survey = survey
        self.isSubmitting = isSubmitting
        self.onBack = onBack
        self.onSubmit = onSubmit
        _answers = State(initialValue: survey.answers ?? [:])
    }

    var body: some View {
        ZStack {
            Color(nsColor: .windowBackgroundColor).ignoresSafeArea()
            LinearGradient(
                colors: [AppPalette.ai.opacity(0.075), AppPalette.ai.opacity(0.025)],
                startPoint: .top,
                endPoint: .bottom
            )
            .ignoresSafeArea()
            ScrollView {
                VStack(alignment: .leading, spacing: 30) {
                    header
                    ForEach(Array(survey.questions.enumerated()), id: \.element.id) { index, question in
                        questionView(question, index: index)
                    }
                    if survey.status == .open {
                        HStack {
                            if !canSubmit {
                                Label("请完成所有必填题", systemImage: "asterisk")
                                    .appFont(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Button {
                                Task { _ = await onSubmit(survey, answers) }
                            } label: {
                                if isSubmitting {
                                    ProgressView().controlSize(.small)
                                } else {
                                    Label("提交并恢复任务", systemImage: "play.fill")
                                }
                            }
                            .buttonStyle(.borderedProminent)
                            .controlSize(.large)
                            .tint(AppPalette.ai)
                            .disabled(isSubmitting || !canSubmit)
                        }
                    } else {
                        Label("答案已保存，原 Local Agent Run 已恢复执行。", systemImage: "checkmark.seal.fill")
                            .appFont(.body.weight(.medium))
                            .foregroundStyle(.green)
                            .padding(14)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(Color.green.opacity(0.08), in: RoundedRectangle(cornerRadius: 10))
                    }
                }
                .padding(.horizontal, 32)
                .padding(.vertical, 30)
                .frame(maxWidth: 820)
                .frame(maxWidth: .infinity)
            }
            .safeAreaInset(edge: .top, spacing: 0) {
                HStack {
                    Button(action: onBack) { Label("返回调研列表", systemImage: "chevron.left") }
                        .buttonStyle(.plain)
                        .foregroundStyle(.secondary)
                    Spacer()
                }
                .padding(.horizontal, 32)
                .padding(.vertical, 14)
                .frame(maxWidth: 820)
                .frame(maxWidth: .infinity)
                .background(.ultraThinMaterial)
                .overlay(alignment: .bottom) { Divider().opacity(0.45) }
            }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 13) {
            HStack {
                Label("本地任务需求调研", systemImage: "list.clipboard")
                    .appFont(.caption.weight(.semibold))
                    .foregroundStyle(AppPalette.ai)
                Text(survey.status == .open ? "等待你的输入" : "已恢复任务")
                    .appFont(.caption.weight(.medium))
                    .foregroundStyle(survey.status == .open ? AppPalette.ai : .green)
            }
            Text(survey.title).appFont(.title2.weight(.bold))
            if let description = survey.description, !description.isEmpty {
                Text(description).appFont(.body).foregroundStyle(.secondary)
            }
            HStack(spacing: 12) {
                Label("Run: \(survey.sourceRunID)", systemImage: "play.circle")
                if let taskID = survey.sourceTaskID {
                    Label("Task: \(taskID)", systemImage: "checklist")
                }
            }
            .appFont(.caption2)
            .foregroundStyle(.tertiary)
            .lineLimit(1)
        }
    }

    @ViewBuilder
    private func questionView(
        _ question: LocalAgentHostRequirementSurveyQuestion,
        index: Int
    ) -> some View {
        VStack(alignment: .leading, spacing: 13) {
            HStack(alignment: .firstTextBaseline, spacing: 5) {
                Text("\(index + 1).")
                Text(question.prompt).fontWeight(.medium)
                if question.required { Text("*").foregroundStyle(.red) }
            }
            .appFont(.title3)

            switch question.responseKind {
            case .text:
                textAnswer(question)
            case .boolean:
                booleanAnswer(question)
            case .singleChoice, .multipleChoice:
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(Array(question.options.enumerated()), id: \.offset) { optionIndex, option in
                        choiceButton(option, optionIndex: optionIndex, question: question)
                    }
                }
            }
        }
    }

    private func textAnswer(_ question: LocalAgentHostRequirementSurveyQuestion) -> some View {
        TextEditor(text: Binding(
            get: {
                guard case let .string(value) = answers[question.id] else { return "" }
                return value
            },
            set: { value in
                if value.isEmpty && !question.required { answers.removeValue(forKey: question.id) }
                else { answers[question.id] = .string(value) }
            }
        ))
        .scrollContentBackground(.hidden)
        .frame(minHeight: 112)
        .padding(10)
        .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 9))
        .overlay { RoundedRectangle(cornerRadius: 9).stroke(AppPalette.border) }
        .disabled(survey.status == .resolved)
    }

    private func booleanAnswer(_ question: LocalAgentHostRequirementSurveyQuestion) -> some View {
        HStack(spacing: 10) {
            booleanButton("是", value: true, question: question)
            booleanButton("否", value: false, question: question)
        }
    }

    private func booleanButton(
        _ title: String,
        value: Bool,
        question: LocalAgentHostRequirementSurveyQuestion
    ) -> some View {
        let selected: Bool = if case let .bool(current) = answers[question.id] { current == value } else { false }
        return Button {
            guard survey.status == .open else { return }
            answers[question.id] = .bool(value)
        } label: {
            Label(title, systemImage: selected ? "checkmark.circle.fill" : "circle")
                .padding(.horizontal, 14)
                .padding(.vertical, 9)
                .background(selected ? AppPalette.ai.opacity(0.1) : AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 9))
                .overlay { RoundedRectangle(cornerRadius: 9).stroke(selected ? AppPalette.ai : AppPalette.border) }
        }
        .buttonStyle(.plain)
        .disabled(survey.status == .resolved)
    }

    private func choiceButton(
        _ option: String,
        optionIndex: Int,
        question: LocalAgentHostRequirementSurveyQuestion
    ) -> some View {
        let selected = selectedOptions(for: question).contains(option)
        return Button {
            guard survey.status == .open else { return }
            if question.responseKind == .singleChoice {
                answers[question.id] = .string(option)
            } else {
                var values = selectedOptions(for: question)
                if selected { values.removeAll { $0 == option } }
                else { values.append(option) }
                if values.isEmpty && !question.required { answers.removeValue(forKey: question.id) }
                else { answers[question.id] = .array(values.map(LocalAgentJSONValue.string)) }
            }
        } label: {
            HStack(spacing: 10) {
                Image(systemName: selected
                    ? (question.responseKind == .multipleChoice ? "checkmark.square.fill" : "record.circle.fill")
                    : (question.responseKind == .multipleChoice ? "square" : "circle"))
                    .foregroundStyle(selected ? AppPalette.ai : .secondary)
                Text(option).appFont(.body).foregroundStyle(.primary)
                Spacer()
                Text(optionLetter(optionIndex)).appFont(.caption2).foregroundStyle(.tertiary)
            }
            .padding(10)
            .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 9))
            .overlay { RoundedRectangle(cornerRadius: 9).stroke(selected ? AppPalette.ai : AppPalette.border) }
        }
        .buttonStyle(.plain)
        .disabled(survey.status == .resolved)
    }

    private func selectedOptions(
        for question: LocalAgentHostRequirementSurveyQuestion
    ) -> [String] {
        switch answers[question.id] {
        case let .string(value): [value]
        case let .array(values): values.compactMap {
            guard case let .string(value) = $0 else { return nil }
            return value
        }
        default: []
        }
    }

    private var canSubmit: Bool {
        survey.questions.allSatisfy { question in
            guard question.required else { return isValidOptionalAnswer(question) }
            return hasValidAnswer(question)
        }
    }

    private func isValidOptionalAnswer(_ question: LocalAgentHostRequirementSurveyQuestion) -> Bool {
        answers[question.id] == nil || hasValidAnswer(question)
    }

    private func hasValidAnswer(_ question: LocalAgentHostRequirementSurveyQuestion) -> Bool {
        switch (question.responseKind, answers[question.id]) {
        case let (.text, .string(value)): !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        case (.boolean, .bool): true
        case let (.singleChoice, .string(value)): question.options.contains(value)
        case let (.multipleChoice, .array(values)):
            !values.isEmpty && values.allSatisfy {
                guard case let .string(value) = $0 else { return false }
                return question.options.contains(value)
            }
        default: false
        }
    }

    private func optionLetter(_ index: Int) -> String {
        guard index >= 0, index < 26 else { return "\(index + 1)" }
        return String(UnicodeScalar(65 + index)!)
    }
}
