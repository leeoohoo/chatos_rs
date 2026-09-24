import Foundation

struct PetTranslationHistoryAttachment: Codable, Equatable, Sendable {
    let name: String
    let mimeType: String
    let isImage: Bool
}

struct PetTranslationHistoryRecord: Codable, Equatable, Identifiable, Sendable {
    let id: UUID
    let createdAt: Date
    let topic: String?
    let sourceText: String
    let attachments: [PetTranslationHistoryAttachment]
    let targetRawValue: String
    let outputStyleRawValue: String
    let modelID: String
    let modelName: String
    let translatedMarkdown: String

    var sourcePreview: String {
        if let topic, !topic.isEmpty { return topic }
        let compactText = sourceText
            .split(whereSeparator: \Character.isWhitespace)
            .joined(separator: " ")
        if !compactText.isEmpty { return compactText }
        if let first = attachments.first {
            return first.isImage ? "图片翻译 · \(first.name)" : "文件翻译 · \(first.name)"
        }
        return "翻译记录"
    }
}

actor PetTranslationHistoryStore {
    static let defaultLimit = 100

    private let fileURL: URL
    private let limit: Int
    private let fileManager: FileManager

    init(
        fileURL: URL,
        limit: Int = PetTranslationHistoryStore.defaultLimit,
        fileManager: FileManager = .default
    ) {
        self.fileURL = fileURL
        self.limit = max(1, limit)
        self.fileManager = fileManager
    }

    func records() throws -> [PetTranslationHistoryRecord] {
        guard fileManager.fileExists(atPath: fileURL.path) else { return [] }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let records = try decoder.decode(
            [PetTranslationHistoryRecord].self,
            from: Data(contentsOf: fileURL)
        )
        return records.sorted { $0.createdAt > $1.createdAt }
    }

    @discardableResult
    func append(_ record: PetTranslationHistoryRecord) throws -> [PetTranslationHistoryRecord] {
        var updated = try records().filter { $0.id != record.id }
        updated.insert(record, at: 0)
        if updated.count > limit {
            updated.removeLast(updated.count - limit)
        }
        try persist(updated)
        return updated
    }

    func clear() throws {
        try persist([])
    }

    private func persist(_ records: [PetTranslationHistoryRecord]) throws {
        try fileManager.createDirectory(
            at: fileURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(records).write(to: fileURL, options: [.atomic])
    }
}
