@testable import ChatOSAgentRuntime
import Foundation
import Testing

@Suite("Agent Attachment Data Loader")
struct AgentAttachmentDataLoaderTests {
    @Test("loads an untruncated text preview within the model input limit")
    func loadsUntruncatedTextPreview() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-attachment-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        let expected = Data("bounded attachment".utf8)
        try expected.write(to: url)

        let loaded = try AgentAttachmentDataLoader.textPreview(url)
        let preview = try #require(loaded)
        #expect(preview.text == String(decoding: expected, as: UTF8.self))
        #expect(!preview.isTruncated)
    }

    @Test("reads only the bounded text prefix and reports truncation")
    func readsBoundedTextPreview() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-attachment-preview-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        let prefix = Data(
            repeating: 0x61,
            count: AgentAttachmentDataLoader.maximumTextPreviewBytes
        )
        var contents = prefix
        contents.append(Data(repeating: 0x62, count: 4 * 1_024 * 1_024))
        try contents.write(to: url)

        let loaded = try AgentAttachmentDataLoader.textPreview(url)
        let preview = try #require(loaded)
        #expect(preview.text.utf8.count == prefix.count)
        #expect(preview.text.allSatisfy { $0 == "a" })
        #expect(preview.isTruncated)
    }

    @Test("rejects symbolic link attachments")
    func rejectsSymbolicLink() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-attachment-link-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let source = root.appendingPathComponent("source")
        let link = root.appendingPathComponent("link")
        try Data("content".utf8).write(to: source)
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: source)

        #expect(throws: AgentRuntimeError.self) {
            _ = try AgentAttachmentDataLoader.textPreview(link)
        }
    }

    @Test("rejects an oversized attachment before reading it")
    func rejectsOversizedAttachment() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-attachment-large-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        #expect(FileManager.default.createFile(atPath: url.path, contents: nil))
        let handle = try FileHandle(forWritingTo: url)
        try handle.truncate(atOffset: UInt64(AgentAttachmentDataLoader.maximumBytes + 1))
        try handle.close()

        #expect(throws: AgentRuntimeError.self) {
            _ = try AgentAttachmentDataLoader.textPreview(url)
        }
    }
}
