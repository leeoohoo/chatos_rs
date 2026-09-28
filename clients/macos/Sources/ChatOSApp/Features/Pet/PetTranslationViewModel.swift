import ChatOSConnector
import ChatOSCore
import Foundation

@MainActor
final class PetTranslationViewModel: ObservableObject {
    private enum PreferenceKey {
        static let target = "ChatOS.petTranslation.target"
        static let outputStyle = "ChatOS.petTranslation.outputStyle"
        static let modelID = "ChatOS.petTranslation.modelID"
    }

    @Published var draft = ""
    @Published var attachments: [ConversationAttachmentDraft] = []
    @Published var attachmentError: String?
    @Published private(set) var result = ""
    @Published private(set) var isTranslating = false
    @Published private(set) var isLoadingModels = false
    @Published private(set) var errorMessage: String?
    @Published private(set) var models: [LocalConnectorModelConfig] = []
    @Published private(set) var petAnimationState: PetAnimationState?
    @Published private(set) var historyRecords: [PetTranslationHistoryRecord] = []
    @Published private(set) var selectedHistoryRecord: PetTranslationHistoryRecord?
    @Published private(set) var isLoadingHistory = false
    @Published private(set) var historyError: String?
    @Published var selectedTarget: PetTranslationTarget {
        didSet { defaults.set(selectedTarget.rawValue, forKey: PreferenceKey.target) }
    }
    @Published var outputStyle: PetTranslationOutputStyle {
        didSet { defaults.set(outputStyle.rawValue, forKey: PreferenceKey.outputStyle) }
    }
    @Published var selectedModelID: String? {
        didSet { defaults.set(selectedModelID, forKey: PreferenceKey.modelID) }
    }

    private let agent: PetTranslationAgent
    private let historyStore: PetTranslationHistoryStore
    private let modelProvider: @MainActor () async throws -> [LocalConnectorModelConfig]
    private let defaults: UserDefaults
    private var translationTask: Task<Void, Never>?
    private var animationResetTask: Task<Void, Never>?
    private var hasLoadedHistory = false
    private var shouldTranslateAfterLoadingModels = false

    init(
        agent: PetTranslationAgent,
        historyStore: PetTranslationHistoryStore,
        defaults: UserDefaults = .standard,
        modelProvider: @escaping @MainActor () async throws -> [LocalConnectorModelConfig]
    ) {
        self.agent = agent
        self.historyStore = historyStore
        self.defaults = defaults
        self.modelProvider = modelProvider
        selectedTarget = PetTranslationTarget(
            rawValue: defaults.string(forKey: PreferenceKey.target) ?? ""
        ) ?? .automatic
        outputStyle = PetTranslationOutputStyle(
            rawValue: defaults.string(forKey: PreferenceKey.outputStyle) ?? ""
        ) ?? .bilingual
        selectedModelID = defaults.string(forKey: PreferenceKey.modelID)
    }

    deinit {
        translationTask?.cancel()
        animationResetTask?.cancel()
    }

    var selectedModel: LocalConnectorModelConfig? {
        guard let selectedModelID else { return nil }
        return models.first(where: { $0.id == selectedModelID })
    }

    var displayedResult: String {
        selectedHistoryRecord?.translatedMarkdown ?? result
    }

    var containsImages: Bool {
        attachments.contains(where: { $0.kind == .image })
    }

    var canTranslate: Bool {
        !isTranslating
            && (!draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                || !attachments.isEmpty)
            && selectedModel != nil
            && (!containsImages || selectedModel?.supportsImages == true)
    }

    func loadModels() {
        guard !isLoadingModels else { return }
        isLoadingModels = true
        Task {
            do {
                let loaded = try await modelProvider()
                models = loaded.sorted {
                    $0.name.localizedStandardCompare($1.name) == .orderedAscending
                }
                normalizeModelSelection()
                if models.isEmpty {
                    errorMessage = "还没有可用的任务模型，请先在设置中配置模型。"
                } else if containsImages && !models.contains(where: \.supportsImages) {
                    errorMessage = "当前没有支持图片输入的模型，请先在设置中配置视觉模型。"
                } else {
                    errorMessage = nil
                }
                if shouldTranslateAfterLoadingModels {
                    shouldTranslateAfterLoadingModels = false
                    translate()
                }
            } catch {
                shouldTranslateAfterLoadingModels = false
                errorMessage = error.localizedDescription
            }
            isLoadingModels = false
        }
    }

    func loadHistory() {
        guard !hasLoadedHistory, !isLoadingHistory else { return }
        isLoadingHistory = true
        Task { [weak self] in
            guard let self else { return }
            do {
                historyRecords = try await historyStore.records()
                hasLoadedHistory = true
                historyError = nil
            } catch {
                historyError = "读取翻译记录失败：\(error.localizedDescription)"
            }
            isLoadingHistory = false
        }
    }

    func selectHistoryRecord(_ record: PetTranslationHistoryRecord?) {
        selectedHistoryRecord = record
    }

    func clearHistory() {
        Task { [weak self] in
            guard let self else { return }
            do {
                try await historyStore.clear()
                historyRecords = []
                selectedHistoryRecord = nil
                historyError = nil
                hasLoadedHistory = true
            } catch {
                historyError = "清空翻译记录失败：\(error.localizedDescription)"
            }
        }
    }

    func translate() {
        guard !isTranslating else { return }
        guard let model = selectedModel else {
            errorMessage = "请先配置并选择一个可用的翻译模型。"
            loadModels()
            return
        }
        guard !containsImages || model.supportsImages else {
            errorMessage = "当前模型不支持图片输入，请选择带“图片”标记的模型。"
            return
        }
        let text = draft
        let outgoingAttachments = attachments
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                || !outgoingAttachments.isEmpty else { return }

        errorMessage = nil
        selectedHistoryRecord = nil
        result = ""
        isTranslating = true
        setPetAnimation(.running)
        let request = PetTranslationRequest(
            text: text,
            attachments: outgoingAttachments,
            target: selectedTarget,
            outputStyle: outputStyle,
            modelConfigID: model.id
        )
        // The request owns immutable copies of the submitted content. Clear the
        // composer immediately so a screenshot sent from the capture toolbar
        // behaves like a true one-click submission while translation continues.
        draft = ""
        attachments = []
        attachmentError = nil
        translationTask = Task { [weak self] in
            guard let self else { return }
            do {
                let completed = try await agent.translate(request) { [weak self] delta in
                    await MainActor.run {
                        self?.result += delta
                    }
                }
                guard !Task.isCancelled else { return }
                result = completed.translatedMarkdown
                let record = PetTranslationHistoryRecord(
                    id: UUID(),
                    createdAt: Date(),
                    topic: completed.historyTitle,
                    sourceText: text.trimmingCharacters(in: .whitespacesAndNewlines),
                    attachments: outgoingAttachments.map {
                        PetTranslationHistoryAttachment(
                            name: $0.name,
                            mimeType: $0.mimeType,
                            isImage: $0.kind == .image
                        )
                    },
                    targetRawValue: request.target.rawValue,
                    outputStyleRawValue: request.outputStyle.rawValue,
                    modelID: model.id,
                    modelName: model.name,
                    translatedMarkdown: completed.translatedMarkdown
                )
                do {
                    historyRecords = try await historyStore.append(record)
                    hasLoadedHistory = true
                    historyError = nil
                } catch {
                    historyError = "保存翻译记录失败：\(error.localizedDescription)"
                }
                setPetAnimation(.succeeded, resetAfter: .seconds(2))
            } catch is CancellationError {
                // Submitted input has already left the composer by design.
            } catch {
                guard !Task.isCancelled else { return }
                errorMessage = error.localizedDescription
                setPetAnimation(.failed, resetAfter: .seconds(2))
            }
            isTranslating = false
            translationTask = nil
        }
    }

    func translateWhenReady() {
        guard !isTranslating else { return }
        if selectedModel != nil {
            translate()
            return
        }
        shouldTranslateAfterLoadingModels = true
        loadModels()
    }

    func cancel() {
        shouldTranslateAfterLoadingModels = false
        translationTask?.cancel()
        translationTask = nil
        isTranslating = false
        setPetAnimation(nil)
    }

    func clear() {
        cancel()
        draft = ""
        attachments = []
        attachmentError = nil
        result = ""
        selectedHistoryRecord = nil
        errorMessage = nil
        setPetAnimation(nil)
    }

    func addAttachmentFiles(_ urls: [URL]) {
        guard !urls.isEmpty else { return }
        attachmentError = nil
        Task {
            let loaded = await Task.detached(priority: .userInitiated) {
                ConversationSessionViewModel.loadAttachmentFiles(urls)
            }.value
            appendAttachments(loaded.attachments, errors: loaded.errors)
        }
    }

    func addPastedImage(data: Data, mimeType: String, suggestedName: String) {
        appendAttachments([
            .init(
                name: suggestedName,
                mimeType: mimeType,
                kind: .image,
                origin: .pastedImage,
                data: data
            ),
        ])
    }

    func addPastedDocument(data: Data, mimeType: String, suggestedName: String) {
        appendAttachments([
            .init(
                name: suggestedName,
                mimeType: mimeType,
                kind: .file,
                origin: .pastedDocument,
                data: data
            ),
        ])
    }

    func addLongPastedText(_ text: String) {
        guard let data = text.data(using: .utf8), !data.isEmpty else { return }
        appendAttachments([
            .init(
                name: ConversationSessionViewModel.pastedName(
                    prefix: "粘贴的长文本",
                    extension: "txt"
                ),
                mimeType: "text/plain",
                kind: .file,
                origin: .pastedText,
                data: data
            ),
        ])
    }

    func removeAttachment(id: String) {
        attachments.removeAll(where: { $0.id == id })
        if attachments.isEmpty { attachmentError = nil }
        normalizeModelSelection()
    }

    private func appendAttachments(
        _ incoming: [ConversationAttachmentDraft],
        errors: [String] = []
    ) {
        var messages = errors
        var totalBytes = attachments.reduce(0) { $0 + $1.size }
        var accepted: [ConversationAttachmentDraft] = []
        for attachment in incoming {
            guard Self.isSupported(attachment) else {
                messages.append("“\(attachment.name)”仅支持图片、PDF 或文本文件")
                continue
            }
            guard attachments.count + accepted.count < 10 else {
                messages.append("单次最多添加 10 个翻译附件")
                break
            }
            guard attachment.size <= 20 * 1_024 * 1_024,
                  totalBytes + attachment.size <= 20 * 1_024 * 1_024 else {
                messages.append("翻译附件总大小不能超过 20 MB")
                continue
            }
            accepted.append(attachment)
            totalBytes += attachment.size
        }
        attachments.append(contentsOf: accepted)
        attachmentError = messages.isEmpty ? nil : messages.joined(separator: "；")
        normalizeModelSelection()
    }

    private func normalizeModelSelection() {
        let compatible = models.filter { !containsImages || $0.supportsImages }
        if let selectedModelID,
           compatible.contains(where: { $0.id == selectedModelID }) {
            return
        }
        selectedModelID = containsImages ? compatible.first?.id : models.first?.id
    }

    private func setPetAnimation(
        _ state: PetAnimationState?,
        resetAfter delay: Duration? = nil
    ) {
        animationResetTask?.cancel()
        animationResetTask = nil
        petAnimationState = state
        guard let delay else { return }
        animationResetTask = Task { [weak self] in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled else { return }
            self?.petAnimationState = nil
            self?.animationResetTask = nil
        }
    }

    private static func isSupported(_ attachment: ConversationAttachmentDraft) -> Bool {
        attachment.kind == .image
            || attachment.mimeType == "application/pdf"
            || attachment.mimeType.hasPrefix("text/")
            || ["application/json", "application/xml", "application/x-yaml"]
                .contains(attachment.mimeType)
    }
}
