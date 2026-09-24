import AppKit
import ChatOSConnector
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

struct PetTranslationView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: PetTranslationViewModel
    let onBack: () -> Void
    let onClose: () -> Void

    @State private var showsFileImporter = false
    @State private var previewedAttachment: ConversationAttachmentDraft?
    @State private var isDropTargeted = false
    @State private var showsHistory = false
    @State private var confirmsClearingHistory = false

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            HStack(spacing: 0) {
                VStack(spacing: 0) {
                    options
                    Divider()
                    content
                    Divider()
                    composer
                }
                if showsHistory {
                    Divider()
                    historySidebar
                        .frame(width: 250)
                        .transition(.move(edge: .trailing).combined(with: .opacity))
                }
            }
        }
        .animation(.easeInOut(duration: 0.18), value: showsHistory)
        .onAppear {
            viewModel.loadModels()
            viewModel.loadHistory()
        }
        .dropDestination(for: URL.self) { urls, _ in
            viewModel.addAttachmentFiles(urls)
            return !urls.isEmpty
        } isTargeted: { isDropTargeted = $0 }
        .overlay {
            if isDropTargeted {
                RoundedRectangle(cornerRadius: 15)
                    .stroke(Color.accentColor, style: StrokeStyle(lineWidth: 2, dash: [7, 5]))
                    .background(Color.accentColor.opacity(0.05), in: RoundedRectangle(cornerRadius: 15))
                    .allowsHitTesting(false)
            }
        }
        .fileImporter(
            isPresented: $showsFileImporter,
            allowedContentTypes: [.image, .pdf, .plainText, .json],
            allowsMultipleSelection: true
        ) { result in
            switch result {
            case let .success(urls):
                viewModel.addAttachmentFiles(urls)
            case let .failure(error):
                viewModel.attachmentError = error.localizedDescription
            }
        }
        .sheet(item: $previewedAttachment) { attachment in
            ComposerAttachmentPreview(attachment: attachment)
        }
        .alert(
            model.localized("清空全部翻译记录？", english: "Clear all translation history?"),
            isPresented: $confirmsClearingHistory
        ) {
            Button(model.localized("取消", english: "Cancel"), role: .cancel) {}
            Button(model.localized("清空", english: "Clear"), role: .destructive) {
                viewModel.clearHistory()
            }
        } message: {
            Text(model.localized(
                "这只会删除本机保存的翻译记录，无法撤销。",
                english: "This deletes locally saved translation records and cannot be undone."
            ))
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Button(action: onBack) {
                Image(systemName: "chevron.left")
                    .frame(width: 26, height: 26)
                    .background(Color(nsColor: .controlBackgroundColor), in: Circle())
            }
            .buttonStyle(.plain)
            Image(systemName: "translate")
                .foregroundStyle(.tint)
            VStack(alignment: .leading, spacing: 2) {
                Text(model.localized("快速翻译", english: "Quick Translate"))
                    .font(.system(size: 14, weight: .semibold))
                Text(model.localized(
                    "独立 Agent · 不调用工具",
                    english: "Independent agent · no tools"
                ))
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button {
                showsHistory.toggle()
            } label: {
                Image(systemName: showsHistory ? "clock.fill" : "clock")
                    .frame(width: 26, height: 26)
                    .foregroundStyle(showsHistory ? Color.accentColor : Color.primary)
                    .background(Color(nsColor: .controlBackgroundColor), in: Circle())
            }
            .buttonStyle(.plain)
            .help(model.localized("翻译记录", english: "Translation History"))
            if !viewModel.displayedResult.isEmpty {
                Button {
                    copyResult()
                } label: {
                    Image(systemName: "doc.on.doc")
                        .frame(width: 26, height: 26)
                        .background(Color(nsColor: .controlBackgroundColor), in: Circle())
                }
                .buttonStyle(.plain)
                .help(model.localized("复制结果", english: "Copy Result"))
            }
            Button(action: onClose) {
                Image(systemName: "xmark")
                    .font(.system(size: 11, weight: .semibold))
                    .frame(width: 26, height: 26)
                    .background(Color(nsColor: .controlBackgroundColor), in: Circle())
            }
            .buttonStyle(.plain)
        }
        .padding(13)
    }

    private var options: some View {
        HStack(spacing: 8) {
            Menu(targetLabel) {
                targetButton(.automatic, zh: "中英自动", en: "Auto Chinese/English")
                targetButton(.simplifiedChinese, zh: "翻译成中文", en: "Translate to Chinese")
                targetButton(.english, zh: "翻译成英文", en: "Translate to English")
            }
            Menu(outputStyleLabel) {
                outputStyleButton(.bilingual, zh: "双语对照", en: "Bilingual")
                outputStyleButton(.translationOnly, zh: "仅译文", en: "Translation Only")
            }
            Menu(modelLabel) {
                ForEach(viewModel.models) { option in
                    Button {
                        viewModel.selectedModelID = option.id
                    } label: {
                        HStack {
                            Text(option.name)
                            if option.supportsImages { Image(systemName: "photo") }
                            if viewModel.selectedModelID == option.id {
                                Image(systemName: "checkmark")
                            }
                        }
                    }
                    .disabled(viewModel.containsImages && !option.supportsImages)
                }
            }
            .disabled(viewModel.models.isEmpty || viewModel.isLoadingModels)
            Spacer()
            if viewModel.isTranslating {
                Button(model.localized("取消", english: "Cancel")) { viewModel.cancel() }
                    .controlSize(.small)
            } else if viewModel.selectedHistoryRecord != nil {
                Button(model.localized("返回当前", english: "Back to Current")) {
                    viewModel.selectHistoryRecord(nil)
                }
                .controlSize(.small)
            } else if !viewModel.result.isEmpty {
                Button(model.localized("清空", english: "Clear")) { viewModel.clear() }
                    .controlSize(.small)
            }
        }
        .font(.system(size: 11))
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }

    @ViewBuilder
    private var content: some View {
        if !viewModel.displayedResult.isEmpty {
            ZStack(alignment: .topLeading) {
                if viewModel.isTranslating && viewModel.selectedHistoryRecord == nil {
                    ScrollView {
                        Text(viewModel.displayedResult)
                            .font(.system(size: 12))
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(12)
                    }
                } else {
                    MarkdownReaderView(markdown: viewModel.displayedResult)
                        .padding(12)
                }
                if viewModel.isTranslating && viewModel.selectedHistoryRecord == nil {
                    HStack(spacing: 6) {
                        ProgressView().controlSize(.small)
                        Text(model.localized("正在翻译…", english: "Translating…"))
                    }
                    .font(.system(size: 10))
                    .padding(8)
                    .background(.regularMaterial, in: Capsule())
                    .padding(10)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            VStack(spacing: 10) {
                Spacer()
                Image(systemName: viewModel.isTranslating ? "ellipsis.bubble" : "doc.text.image")
                    .font(.system(size: 28))
                    .foregroundStyle(Color.accentColor)
                Text(viewModel.isTranslating
                     ? model.localized("正在识别并翻译", english: "Reading and translating")
                     : model.localized("粘贴文字或截图", english: "Paste text or a screenshot"))
                    .font(.system(size: 13, weight: .semibold))
                Text(model.localized(
                    "也可以把图片、PDF 或文本文件拖到这里",
                    english: "You can also drop images, PDFs, or text files here"
                ))
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                if viewModel.isTranslating { ProgressView().controlSize(.small) }
                Spacer()
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color(nsColor: .textBackgroundColor).opacity(0.24))
        }
    }

    private var composer: some View {
        VStack(alignment: .leading, spacing: 7) {
            if !viewModel.attachments.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 7) {
                        ForEach(viewModel.attachments) { attachment in
                            ComposerAttachmentChip(
                                attachment: attachment,
                                onPreview: { previewedAttachment = attachment },
                                onRemove: { viewModel.removeAttachment(id: attachment.id) }
                            )
                        }
                    }
                }
            }
            if let message = viewModel.attachmentError ?? viewModel.errorMessage {
                Text(message)
                    .font(.system(size: 10))
                    .foregroundStyle(.red)
                    .lineLimit(2)
            }
            HStack(alignment: .bottom, spacing: 8) {
                Button {
                    showsFileImporter = true
                } label: {
                    Image(systemName: "paperclip")
                        .frame(width: 28, height: 28)
                }
                .buttonStyle(.plain)
                .help(model.localized("添加图片或文件", english: "Add Image or File"))

                ComposerPasteTextEditor(
                    text: $viewModel.draft,
                    placeholder: model.localized(
                        "输入文字，或直接粘贴截图…",
                        english: "Type text, or paste a screenshot…"
                    ),
                    onSubmit: viewModel.translate,
                    onPasteContent: handlePasteContent
                )

                Button(action: viewModel.translate) {
                    Group {
                        if viewModel.isTranslating {
                            ProgressView().controlSize(.small)
                        } else {
                            Image(systemName: "translate")
                                .font(.system(size: 12, weight: .bold))
                        }
                    }
                    .frame(width: 30, height: 30)
                    .foregroundStyle(viewModel.canTranslate ? Color.white : Color.secondary)
                    .background(
                        viewModel.canTranslate ? Color.accentColor : Color.secondary.opacity(0.14),
                        in: Circle()
                    )
                }
                .buttonStyle(.plain)
                .disabled(!viewModel.canTranslate)
                .help(model.localized("翻译", english: "Translate"))
            }
        }
        .padding(11)
    }

    private var historySidebar: some View {
        VStack(spacing: 0) {
            HStack {
                Text(model.localized("翻译记录", english: "Translation History"))
                    .font(.system(size: 12, weight: .semibold))
                Spacer()
                if !viewModel.historyRecords.isEmpty {
                    Button {
                        confirmsClearingHistory = true
                    } label: {
                        Image(systemName: "trash")
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(.secondary)
                    .help(model.localized("清空记录", english: "Clear History"))
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 11)
            Divider()

            if viewModel.isLoadingHistory {
                Spacer()
                ProgressView().controlSize(.small)
                Spacer()
            } else if viewModel.historyRecords.isEmpty {
                Spacer()
                VStack(spacing: 7) {
                    Image(systemName: "clock.arrow.circlepath")
                        .font(.system(size: 22))
                        .foregroundStyle(.secondary)
                    Text(model.localized("还没有翻译记录", english: "No translations yet"))
                        .font(.system(size: 11, weight: .medium))
                    Text(model.localized(
                        "完成翻译后会自动保存在这里",
                        english: "Completed translations appear here"
                    ))
                        .font(.system(size: 10))
                        .foregroundStyle(.secondary)
                }
                Spacer()
            } else {
                ScrollView {
                    LazyVStack(spacing: 5) {
                        ForEach(viewModel.historyRecords) { record in
                            historyRow(record)
                        }
                    }
                    .padding(7)
                }
            }

            Divider()
            VStack(alignment: .leading, spacing: 3) {
                if let historyError = viewModel.historyError {
                    Text(historyError)
                        .foregroundStyle(.red)
                        .lineLimit(2)
                } else {
                    Text(model.localized(
                        "仅保存在本机 · 最多 100 条",
                        english: "Stored locally · Up to 100 records"
                    ))
                    Text(model.localized(
                        "不保存图片或文件本体，不作为 Agent 记忆",
                        english: "Files are not saved or used as agent memory"
                    ))
                }
            }
            .font(.system(size: 9))
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(9)
        }
        .background(Color(nsColor: .windowBackgroundColor).opacity(0.7))
    }

    private func historyRow(_ record: PetTranslationHistoryRecord) -> some View {
        Button {
            viewModel.selectHistoryRecord(record)
        } label: {
            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 5) {
                    Image(systemName: record.attachments.contains(where: \.isImage)
                          ? "photo" : "text.alignleft")
                        .font(.system(size: 9))
                        .foregroundStyle(.secondary)
                    Text(record.sourcePreview)
                        .font(.system(size: 11, weight: .medium))
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                    Spacer(minLength: 0)
                }
                HStack(spacing: 5) {
                    Text(historyModeLabel(record))
                    Text("·")
                    Text(record.modelName)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    Text(record.createdAt, format: .dateTime.month().day().hour().minute())
                }
                .font(.system(size: 9))
                .foregroundStyle(.secondary)
            }
            .padding(8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                viewModel.selectedHistoryRecord?.id == record.id
                    ? Color.accentColor.opacity(0.14)
                    : Color(nsColor: .controlBackgroundColor).opacity(0.65),
                in: RoundedRectangle(cornerRadius: 8)
            )
        }
        .buttonStyle(.plain)
    }

    private func historyModeLabel(_ record: PetTranslationHistoryRecord) -> String {
        let target = switch record.targetRawValue {
        case PetTranslationTarget.simplifiedChinese.rawValue:
            model.localized("译中", english: "To Chinese")
        case PetTranslationTarget.english.rawValue:
            model.localized("译英", english: "To English")
        default:
            model.localized("自动", english: "Auto")
        }
        let style = record.outputStyleRawValue == PetTranslationOutputStyle.bilingual.rawValue
            ? model.localized("双语", english: "Bilingual")
            : model.localized("仅译文", english: "Translation")
        return "\(target) · \(style)"
    }

    private var targetLabel: String {
        switch viewModel.selectedTarget {
        case .automatic: model.localized("中英自动", english: "Auto")
        case .simplifiedChinese: model.localized("译成中文", english: "To Chinese")
        case .english: model.localized("译成英文", english: "To English")
        }
    }

    private var outputStyleLabel: String {
        switch viewModel.outputStyle {
        case .bilingual: model.localized("双语对照", english: "Bilingual")
        case .translationOnly: model.localized("仅译文", english: "Translation Only")
        }
    }

    private var modelLabel: String {
        if viewModel.isLoadingModels {
            return model.localized("读取模型…", english: "Loading Models…")
        }
        return viewModel.selectedModel?.name
            ?? model.localized("选择模型", english: "Choose Model")
    }

    private func targetButton(_ target: PetTranslationTarget, zh: String, en: String) -> some View {
        Button {
            viewModel.selectedTarget = target
        } label: {
            if viewModel.selectedTarget == target {
                Label(model.localized(zh, english: en), systemImage: "checkmark")
            } else {
                Text(model.localized(zh, english: en))
            }
        }
    }

    private func outputStyleButton(
        _ style: PetTranslationOutputStyle,
        zh: String,
        en: String
    ) -> some View {
        Button {
            viewModel.outputStyle = style
        } label: {
            if viewModel.outputStyle == style {
                Label(model.localized(zh, english: en), systemImage: "checkmark")
            } else {
                Text(model.localized(zh, english: en))
            }
        }
    }

    private func handlePasteContent(_ content: ComposerPasteContent) {
        switch content {
        case let .files(urls):
            viewModel.addAttachmentFiles(urls)
        case let .image(data, mimeType, suggestedName):
            viewModel.addPastedImage(
                data: data,
                mimeType: mimeType,
                suggestedName: suggestedName
            )
        case let .document(data, mimeType, suggestedName):
            viewModel.addPastedDocument(
                data: data,
                mimeType: mimeType,
                suggestedName: suggestedName
            )
        case let .longText(text):
            viewModel.addLongPastedText(text)
        }
    }

    private func copyResult() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(viewModel.displayedResult, forType: .string)
    }
}
