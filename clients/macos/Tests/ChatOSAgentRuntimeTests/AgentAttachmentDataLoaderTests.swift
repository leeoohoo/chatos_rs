@testable import ChatOSAgentRuntime
import Foundation
import Testing

@Suite("Agent Attachment Data Loader")
struct AgentAttachmentDataLoaderTests {
    @Test("loads an attachment within the model input limit")
    func loadsBoundedAttachment() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-attachment-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        let expected = Data("bounded attachment".utf8)
        try expected.write(to: url)

        #expect(try AgentAttachmentDataLoader.load(url) == expected)
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
            _ = try AgentAttachmentDataLoader.load(url)
        }
    }
}
