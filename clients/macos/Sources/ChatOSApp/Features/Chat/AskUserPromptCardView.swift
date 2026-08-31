import ChatOSCore
import SwiftUI

struct AskUserPromptCardView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var conversation: ConversationSessionViewModel
    let prompt: AskUserPrompt

    @State private var values: [String: String]
    @State private var selection: Set<String>

    init(conversation: ConversationSessionViewModel, prompt: AskUserPrompt) {
        self.conversation = conversation
        self.prompt = prompt
        _values = State(initialValue: Dictionary(
            uniqueKeysWithValues: prompt.fields.map { ($0.key, $0.defaultValue) }
        ))
        _selection = State(initialValue: Set(prompt.choice?.defaultSelection ?? []))
    }

    var body: some View {
        Group {
            if prompt.status.isPending {
                pendingCard
            } else {
                resolvedCard
            }
        }
        .onChange(of: prompt.status) {
            guard !prompt.status.isPending else { return }
            for field in prompt.fields where field.isSecret {
                values[field.key] = ""
            }
        }
    }

    private var pendingCard: some View {
        VStack(alignment: .leading, spacing: 16) {
            header

            if !prompt.message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                MarkdownDocumentView(
                    markdown: prompt.message,
                    allowsTextSelection: false
                )
                    .foregroundStyle(.secondary)
            }

            if !prompt.fields.isEmpty {
                fieldsForm
            }

            if let choice = prompt.choice {
                choiceForm(choice)
            }

            if let error = conversation.askUserPromptErrors[prompt.id] {
                Label(error, systemImage: "exclamationmark.triangle.fill")
                    .appFont(.caption)
                    .foregroundStyle(.red)
            }

            actions
        }
        .padding(18)
        .frame(maxWidth: 760, alignment: .leading)
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 16))
        .overlay {
            RoundedRectangle(cornerRadius: 16)
                .stroke(AppPalette.ai.opacity(0.30), lineWidth: 1)
        }
        .shadow(color: AppPalette.ai.opacity(0.08), radius: 12, y: 4)
    }

    private var header: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "questionmark.bubble.fill")
                .appFont(.system(size: 18, weight: .semibold))
                .foregroundStyle(AppPalette.ai)
                .frame(width: 34, height: 34)
                .background(AppPalette.aiSoft, in: RoundedRectangle(cornerRadius: 10))

            VStack(alignment: .leading, spacing: 4) {
                Text(prompt.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                     ? model.localized("需要你的输入", english: "Your input is needed")
                     : prompt.title)
                    .appFont(.headline)
                Text(model.localized("任务正在等待你的答复", english: "The task is waiting for your response"))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 12)
            StatusCapsule(title: model.localized("待处理", english: "Pending"), color: AppPalette.ai)
        }
    }

    private var fieldsForm: some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 260), spacing: 14, alignment: .top)],
            alignment: .leading,
            spacing: 14
        ) {
            ForEach(prompt.fields) { field in
                VStack(alignment: .leading, spacing: 7) {
                    HStack(spacing: 3) {
                        Text(field.label).appFont(.subheadline.weight(.medium))
                        if field.isRequired {
                            Text("*").foregroundStyle(.red)
                        }
                    }
                    if let description = field.description {
                        Text(description)
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                    }
                    fieldControl(field)
                }
            }
        }
    }

    @ViewBuilder
    private func fieldControl(_ field: AskUserField) -> some View {
        if field.isSecret {
            SecureField(field.placeholder ?? "", text: binding(for: field.key))
                .textFieldStyle(.plain)
                .askUserInputStyle()
        } else if field.isMultiline {
            ZStack(alignment: .topLeading) {
                if values[field.key, default: ""].isEmpty,
                   let placeholder = field.placeholder,
                   !placeholder.isEmpty {
                    Text(placeholder)
                        .appFont(.body)
                        .foregroundStyle(.tertiary)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 8)
                }
                TextEditor(text: binding(for: field.key))
                    .scrollContentBackground(.hidden)
                    .appFont(.body)
                    .frame(minHeight: 86)
            }
            .askUserInputStyle()
        } else {
            TextField(field.placeholder ?? "", text: binding(for: field.key))
                .textFieldStyle(.plain)
                .askUserInputStyle()
        }
    }

    private func choiceForm(_ choice: AskUserChoice) -> some View {
        VStack(alignment: .leading, spacing: 9) {
            HStack(spacing: 6) {
                Text(choice.allowsMultiple
                     ? model.localized("请选择（可多选）", english: "Select one or more")
                     : model.localized("请选择", english: "Select one"))
                    .appFont(.subheadline.weight(.medium))
                if choice.minimumSelectionCount > 0 {
                    Text(selectionHint(choice))
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 240), spacing: 10)],
                alignment: .leading,
                spacing: 10
            ) {
                ForEach(choice.options) { option in
                    choiceButton(option, choice: choice)
                }
            }
        }
    }

    private func choiceButton(_ option: AskUserChoiceOption, choice: AskUserChoice) -> some View {
        let selected = selection.contains(option.value)
        return Button {
            toggle(option.value, in: choice)
        } label: {
            HStack(alignment: .top, spacing: 10) {
                Image(systemName: choice.allowsMultiple
                      ? (selected ? "checkmark.square.fill" : "square")
                      : (selected ? "circle.inset.filled" : "circle"))
                    .foregroundStyle(selected ? AppPalette.ai : .secondary)
                    .appFont(.system(size: 16, weight: .medium))
                VStack(alignment: .leading, spacing: 3) {
                    Text(option.label)
                        .appFont(.subheadline.weight(.medium))
                        .foregroundStyle(.primary)
                    if let description = option.description {
                        Text(description)
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(11)
            .frame(maxWidth: .infinity, minHeight: 48, alignment: .leading)
            .background(
                selected ? AppPalette.ai.opacity(0.08) : AppPalette.inputSurface,
                in: RoundedRectangle(cornerRadius: 11)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 11)
                    .stroke(selected ? AppPalette.ai.opacity(0.55) : AppPalette.border, lineWidth: 1)
            }
        }
        .buttonStyle(.plain)
    }

    private var actions: some View {
        HStack(spacing: 10) {
            if prompt.allowsCancel {
                Button(model.localized("取消请求", english: "Cancel Request")) {
                    conversation.cancelAskUserPrompt(prompt)
                }
                .buttonStyle(.bordered)
                .disabled(isSubmitting)
            }
            Spacer()
            Button {
                conversation.submitAskUserPrompt(prompt, submission: submission)
            } label: {
                if isSubmitting {
                    ProgressView().controlSize(.small)
                } else {
                    Label(model.localized("确认提交", english: "Submit"), systemImage: "checkmark")
                }
            }
            .buttonStyle(.borderedProminent)
            .tint(AppPalette.ai)
            .disabled(!isValid || isSubmitting)
        }
    }

    private var resolvedCard: some View {
        HStack(spacing: 11) {
            Image(systemName: resolvedIcon)
                .foregroundStyle(resolvedColor)
                .appFont(.system(size: 16, weight: .semibold))
            VStack(alignment: .leading, spacing: 2) {
                Text(prompt.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                     ? model.localized("交互请求", english: "Interactive Request")
                     : prompt.title)
                    .appFont(.subheadline.weight(.medium))
                Text(resolvedTitle)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 11)
        .frame(maxWidth: 760, alignment: .leading)
        .background(resolvedColor.opacity(0.06), in: RoundedRectangle(cornerRadius: 12))
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .stroke(resolvedColor.opacity(0.18), lineWidth: 1)
        }
    }

    private var submission: AskUserSubmission {
        let choiceSelection: AskUserSelection?
        if let choice = prompt.choice {
            let ordered = choice.options.map(\.value).filter(selection.contains)
            choiceSelection = choice.allowsMultiple
                ? .multiple(ordered)
                : .single(ordered.first ?? "")
        } else {
            choiceSelection = nil
        }
        return AskUserSubmission(values: values, selection: choiceSelection)
    }

    private var isValid: Bool {
        for field in prompt.fields where field.isRequired {
            if values[field.key, default: ""].trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                return false
            }
        }
        if let choice = prompt.choice {
            let count = selection.count
            if count < choice.minimumSelectionCount || count > choice.maximumSelectionCount {
                return false
            }
        }
        return true
    }

    private var isSubmitting: Bool {
        conversation.isSubmitting(promptID: prompt.id)
    }

    private func binding(for key: String) -> Binding<String> {
        Binding(
            get: { values[key, default: ""] },
            set: { values[key] = $0 }
        )
    }

    private func toggle(_ value: String, in choice: AskUserChoice) {
        if !choice.allowsMultiple {
            selection = [value]
        } else if selection.contains(value) {
            selection.remove(value)
        } else if selection.count < choice.maximumSelectionCount {
            selection.insert(value)
        }
    }

    private func selectionHint(_ choice: AskUserChoice) -> String {
        if choice.minimumSelectionCount == choice.maximumSelectionCount {
            return model.localized(
                "选择 \(choice.minimumSelectionCount) 项",
                english: "Select \(choice.minimumSelectionCount)"
            )
        }
        return model.localized(
            "选择 \(choice.minimumSelectionCount)–\(choice.maximumSelectionCount) 项",
            english: "Select \(choice.minimumSelectionCount)–\(choice.maximumSelectionCount)"
        )
    }

    private var resolvedTitle: String {
        switch prompt.status {
        case .ok: model.localized("已提交，任务将继续执行", english: "Submitted. The task will continue.")
        case .canceled: model.localized("已取消", english: "Canceled")
        case .timeout: model.localized("已超时", english: "Timed out")
        case .failed: model.localized("处理失败", english: "Failed")
        case .pending: model.localized("待处理", english: "Pending")
        }
    }

    private var resolvedIcon: String {
        switch prompt.status {
        case .ok: "checkmark.circle.fill"
        case .canceled: "xmark.circle.fill"
        case .timeout: "clock.badge.exclamationmark.fill"
        case .failed: "exclamationmark.triangle.fill"
        case .pending: "questionmark.circle.fill"
        }
    }

    private var resolvedColor: Color {
        switch prompt.status {
        case .ok: .green
        case .canceled, .timeout: .secondary
        case .failed: .red
        case .pending: AppPalette.ai
        }
    }
}

private extension View {
    func askUserInputStyle() -> some View {
        padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 10))
            .overlay {
                RoundedRectangle(cornerRadius: 10)
                    .stroke(AppPalette.border, lineWidth: 1)
            }
    }
}
