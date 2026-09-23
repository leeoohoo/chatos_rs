import ChatOSCore
import SwiftUI

struct PetAskUserInlineView: View {
    @EnvironmentObject private var model: AppModel
    let activity: PetActivity
    let onLoadPrompt: (PetActivity) async throws -> AskUserPrompt
    let onSubmitPrompt: (AskUserPrompt, AskUserSubmission) async throws -> Void
    let onCancelPrompt: (AskUserPrompt) async throws -> Void
    let onResolved: () -> Void

    @State private var prompt: AskUserPrompt?
    @State private var values: [String: String] = [:]
    @State private var selection: Set<String> = []
    @State private var isLoading = true
    @State private var isSubmitting = false
    @State private var errorMessage: String?

    var body: some View {
        Group {
            if isLoading {
                VStack(spacing: 10) {
                    ProgressView()
                    Text("正在加载输入内容…")
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding(20)
            } else if let prompt {
                promptContent(prompt)
            } else {
                VStack(alignment: .leading, spacing: 10) {
                    Label(errorMessage ?? model.localized("这个输入请求已结束。", english: "This input request has ended."), systemImage: "exclamationmark.triangle.fill")
                        .font(.system(size: 12))
                        .foregroundStyle(.orange)
                    HStack {
                        Spacer()
                        Button("重新加载") { load() }
                            .controlSize(.small)
                    }
                }
                .padding(13)
            }
        }
        .task(id: activity.id) {
            await loadPrompt()
        }
    }

    private func promptContent(_ prompt: AskUserPrompt) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    if !prompt.message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        Text(prompt.message)
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    ForEach(prompt.fields) { field in
                        VStack(alignment: .leading, spacing: 5) {
                            HStack(spacing: 3) {
                                Text(field.label)
                                    .font(.system(size: 11, weight: .medium))
                                if field.isRequired {
                                    Text("*").foregroundStyle(.red)
                                }
                            }
                            if let description = field.description,
                               !description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                                Text(description)
                                    .font(.system(size: 10))
                                    .foregroundStyle(.secondary)
                            }
                            fieldControl(field)
                        }
                    }

                    if let choice = prompt.choice {
                        choiceControl(choice)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            if let errorMessage {
                Text(errorMessage)
                    .font(.system(size: 11))
                    .foregroundStyle(.red)
                    .lineLimit(3)
            }

            Divider()
            HStack(spacing: 8) {
                if prompt.allowsCancel {
                    Button("取消请求", role: .destructive) { cancel(prompt) }
                }
                Spacer()
                Button {
                    submit(prompt)
                } label: {
                    if isSubmitting {
                        ProgressView().controlSize(.small)
                    } else {
                        Label("提交", systemImage: "checkmark")
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(!isValid(prompt) || isSubmitting)
            }
            .controlSize(.small)
        }
        .padding(13)
    }

    @ViewBuilder
    private func fieldControl(_ field: AskUserField) -> some View {
        if field.isSecret {
            SecureField(field.placeholder ?? "", text: binding(for: field.key))
                .textFieldStyle(.roundedBorder)
        } else if field.isMultiline {
            TextEditor(text: binding(for: field.key))
                .font(.system(size: 11))
                .scrollContentBackground(.hidden)
                .padding(4)
                .frame(minHeight: 62)
                .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 7))
                .overlay { RoundedRectangle(cornerRadius: 7).stroke(.separator) }
        } else {
            TextField(field.placeholder ?? "", text: binding(for: field.key))
                .textFieldStyle(.roundedBorder)
        }
    }

    private func choiceControl(_ choice: AskUserChoice) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(choice.allowsMultiple
                 ? model.localized("请选择（可多选）", english: "Select one or more")
                 : model.localized("请选择", english: "Select"))
                .font(.system(size: 11, weight: .medium))
            ForEach(choice.options) { option in
                let selected = selection.contains(option.value)
                Button {
                    toggle(option.value, in: choice)
                } label: {
                    HStack(alignment: .top, spacing: 7) {
                        Image(systemName: choice.allowsMultiple
                              ? (selected ? "checkmark.square.fill" : "square")
                              : (selected ? "circle.inset.filled" : "circle"))
                            .foregroundStyle(selected ? Color.accentColor : Color.secondary)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(option.label)
                                .font(.system(size: 11, weight: .medium))
                            if let description = option.description {
                                Text(description)
                                    .font(.system(size: 10))
                                    .foregroundStyle(.secondary)
                            }
                        }
                        Spacer()
                    }
                    .padding(7)
                    .background(
                        selected ? Color.accentColor.opacity(0.09) : Color(nsColor: .controlBackgroundColor),
                        in: RoundedRectangle(cornerRadius: 8)
                    )
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func load() {
        Task { await loadPrompt() }
    }

    private func loadPrompt() async {
        isLoading = true
        errorMessage = nil
        do {
            let loaded = try await onLoadPrompt(activity)
            guard !Task.isCancelled else { return }
            prompt = loaded
            values = Dictionary(uniqueKeysWithValues: loaded.fields.map { ($0.key, $0.defaultValue) })
            selection = Set(loaded.choice?.defaultSelection ?? [])
        } catch {
            guard !Task.isCancelled else { return }
            prompt = nil
            errorMessage = error.localizedDescription
        }
        isLoading = false
    }

    private func submit(_ prompt: AskUserPrompt) {
        guard !isSubmitting, isValid(prompt) else { return }
        isSubmitting = true
        errorMessage = nil
        Task {
            do {
                try await onSubmitPrompt(prompt, submission(prompt))
                clearSecrets(prompt)
                onResolved()
            } catch {
                errorMessage = error.localizedDescription
            }
            isSubmitting = false
        }
    }

    private func cancel(_ prompt: AskUserPrompt) {
        guard !isSubmitting else { return }
        isSubmitting = true
        errorMessage = nil
        Task {
            do {
                try await onCancelPrompt(prompt)
                clearSecrets(prompt)
                onResolved()
            } catch {
                errorMessage = error.localizedDescription
            }
            isSubmitting = false
        }
    }

    private func submission(_ prompt: AskUserPrompt) -> AskUserSubmission {
        let selectedValues = prompt.choice?.options.map(\.value).filter(selection.contains) ?? []
        let answer: AskUserSelection?
        if let choice = prompt.choice {
            answer = choice.allowsMultiple
                ? .multiple(selectedValues)
                : .single(selectedValues.first ?? "")
        } else {
            answer = nil
        }
        return AskUserSubmission(values: values, selection: answer)
    }

    private func isValid(_ prompt: AskUserPrompt) -> Bool {
        for field in prompt.fields where field.isRequired {
            if values[field.key, default: ""].trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                return false
            }
        }
        if let choice = prompt.choice {
            return selection.count >= choice.minimumSelectionCount
                && selection.count <= choice.maximumSelectionCount
        }
        return true
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

    private func clearSecrets(_ prompt: AskUserPrompt) {
        for field in prompt.fields where field.isSecret {
            values[field.key] = ""
        }
    }
}
