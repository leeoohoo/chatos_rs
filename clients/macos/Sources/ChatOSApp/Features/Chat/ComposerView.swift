import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

struct ComposerView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var conversation: ConversationSessionViewModel
    @State private var showsFileImporter = false
    @State private var previewedAttachment: ConversationAttachmentDraft?
    @State private var isDropTargeted = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            controls
            attachmentStrip
            attachmentError
            input
        }
        .padding(14)
        .background(AppPalette.surfaceSubtle, in: RoundedRectangle(cornerRadius: 13))
        .overlay {
            RoundedRectangle(cornerRadius: 13).stroke(AppPalette.border, lineWidth: 1)
        }
        .shadow(color: .black.opacity(0.055), radius: 10, y: 3)
        .overlay {
            if isDropTargeted {
                RoundedRectangle(cornerRadius: 13)
                    .stroke(AppPalette.ai, style: StrokeStyle(lineWidth: 2, dash: [7, 5]))
                    .background(AppPalette.ai.opacity(0.05), in: RoundedRectangle(cornerRadius: 13))
                    .allowsHitTesting(false)
            }
        }
        .dropDestination(for: URL.self) { urls, _ in
            conversation.addAttachmentFiles(urls)
            return !urls.isEmpty
        } isTargeted: { targeted in
            isDropTargeted = targeted
        }
        .fileImporter(
            isPresented: $showsFileImporter,
            allowedContentTypes: [.item],
            allowsMultipleSelection: true
        ) { result in
            switch result {
            case let .success(urls):
                conversation.addAttachmentFiles(urls)
            case let .failure(error):
                conversation.attachmentError = error.localizedDescription
            }
        }
        .sheet(item: $previewedAttachment) { attachment in
            ComposerAttachmentPreview(attachment: attachment)
        }
    }

    private var controls: some View {
        HStack(spacing: 8) {
            Menu(selectedModelDisplayName) {
                ForEach(conversation.availableModels) { model in
                    Button {
                        conversation.setSelectedModelID(model.id)
                    } label: {
                        if conversation.selectedModelID == model.id {
                            Label(model.displayName, systemImage: "checkmark")
                        } else {
                            Text(model.displayName)
                        }
                    }
                }
            }
            .disabled(conversation.availableModels.isEmpty || conversation.isUpdatingRuntimeSettings)
            Button(model.localized("附件", english: "Attachments"), systemImage: "paperclip") {
                showsFileImporter = true
            }
                .labelStyle(.iconOnly)
                .help(model.localized(
                    "添加图片、文档或其他文件；也可以直接粘贴或拖入",
                    english: "Add images, documents, or other files. You can also paste or drag them here."
                ))
            if conversation.allowsPlanMode {
                Button(model.localized(
                    "规划 \(conversation.planModeEnabled ? "开" : "关")",
                    english: "Plan \(conversation.planModeEnabled ? "On" : "Off")"
                )) {
                    conversation.setPlanModeEnabled(!conversation.planModeEnabled)
                }
                .buttonStyle(.borderedProminent)
                .tint(conversation.planModeEnabled ? AppPalette.ai : AppPalette.idleControl)
                .help(
                    conversation.planModeEnabled
                        ? model.localized(
                            "开启后，AI 会先通过 Task Runner 生成待确认的任务图。",
                            english: "When enabled, AI first creates a task graph for confirmation through Task Runner."
                        )
                        : model.localized(
                            "关闭后，AI 可直接创建并执行任务。",
                            english: "When disabled, AI can create and execute tasks directly."
                        )
                )
                .disabled(conversation.isUpdatingRuntimeSettings)
            }
            Button(model.localized(
                "推理 \(conversation.reasoningEnabled ? "开" : "关")",
                english: "Reasoning \(conversation.reasoningEnabled ? "On" : "Off")"
            )) {
                conversation.setReasoningEnabled(!conversation.reasoningEnabled)
            }
            .buttonStyle(.borderedProminent)
            .tint(conversation.reasoningEnabled ? AppPalette.ai : AppPalette.idleControl)
            .disabled(conversation.isUpdatingRuntimeSettings)
            Spacer()
        }
        .controlSize(.small)
    }

    private var selectedModelDisplayName: String {
        if let selected = conversation.availableModels.first(where: {
            $0.id == conversation.selectedModelID
        }) {
            return selected.displayName
        }
        if let first = conversation.availableModels.first {
            return first.displayName
        }
        return model.localized("选择模型", english: "Select Model")
    }

    @ViewBuilder
    private var attachmentStrip: some View {
        if !conversation.attachments.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(conversation.attachments) { attachment in
                        ComposerAttachmentChip(
                            attachment: attachment,
                            onPreview: { previewedAttachment = attachment },
                            onRemove: { conversation.removeAttachment(id: attachment.id) }
                        )
                    }
                }
                .padding(.horizontal, 1)
            }
        }
    }

    @ViewBuilder
    private var attachmentError: some View {
        if let error = conversation.attachmentError {
            HStack(alignment: .top, spacing: 7) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                Text(error)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Button(model.localized("关闭", english: "Dismiss"), systemImage: "xmark") {
                    conversation.clearAttachmentError()
                }
                .labelStyle(.iconOnly)
                .buttonStyle(.plain)
            }
        }
    }

    private var input: some View {
        HStack(alignment: .bottom, spacing: 10) {
            ComposerPasteTextEditor(
                text: $conversation.draft,
                placeholder: model.localized(
                    "输入消息，或粘贴图片、文档和长文本…",
                    english: "Type a message, or paste images, documents, and long text…"
                ),
                onSubmit: conversation.sendDraft,
                onPasteContent: handlePasteContent
            )

            Button(action: conversation.sendDraft) {
                Group {
                    if conversation.isSending {
                        ProgressView().controlSize(.small)
                    } else {
                        Image(systemName: "arrow.up").appFont(.headline)
                    }
                }
                .frame(width: 30, height: 30)
            }
            .buttonStyle(.borderedProminent)
            .buttonBorderShape(.circle)
            .disabled(!conversation.canSendDraft)
        }
        .padding(.leading, 13)
        .padding(.trailing, 7)
        .padding(.vertical, 4)
        .background(AppPalette.inputSurface, in: RoundedRectangle(cornerRadius: 11))
        .overlay {
            RoundedRectangle(cornerRadius: 11)
                .stroke(AppPalette.ai.opacity(0.24), lineWidth: 1)
        }
    }

    private func handlePasteContent(_ content: ComposerPasteContent) {
        switch content {
        case let .files(urls):
            conversation.addAttachmentFiles(urls)
        case let .image(data, mimeType, suggestedName):
            conversation.addPastedImage(
                data: data,
                mimeType: mimeType,
                suggestedName: suggestedName
            )
        case let .document(data, mimeType, suggestedName):
            conversation.addPastedDocument(
                data: data,
                mimeType: mimeType,
                suggestedName: suggestedName
            )
        case let .longText(text):
            conversation.addLongPastedText(text)
        }
    }
}
