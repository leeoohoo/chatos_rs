import Foundation
import Testing
@testable import ChatOSAgentRuntime

@Suite("Agent Model Request Body File Writer")
struct AgentModelRequestBodyFileWriterTests {
    @Test("incrementally replaces multiple attachment placeholders")
    func writesMultipleSources() throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let image = Data(repeating: 0x5a, count: 200 * 1_024 + 1)
        let pdf = Data("%PDF-1.7\nfixture".utf8)
        let imageURL = fixture.root.appendingPathComponent("image.bin")
        let pdfURL = fixture.root.appendingPathComponent("document.pdf")
        try image.write(to: imageURL)
        try pdf.write(to: pdfURL)

        let body = try AgentModelRequestBodyFileWriter.write(
            payload: [
                "input": [
                    ["image_url": "image-placeholder"],
                    ["file_data": "pdf-placeholder"],
                ],
            ],
            sources: [
                .init(
                    placeholder: "image-placeholder",
                    url: imageURL,
                    mimeType: "image/png"
                ),
                .init(
                    placeholder: "pdf-placeholder",
                    url: pdfURL,
                    mimeType: "application/pdf"
                ),
            ],
            temporaryDirectory: fixture.root,
            availableCapacityOverride: Int64.max
        )
        defer { body.removeIfNeeded() }

        let data = try Data(contentsOf: body.url)
        #expect(data.count == body.byteCount)
        let decoded = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let payload = try #require(decoded)
        let input = try #require(payload["input"] as? [[String: String]])
        #expect(
            input[0]["image_url"]
                == "data:image/png;base64,\(image.base64EncodedString())"
        )
        #expect(
            input[1]["file_data"]
                == "data:application/pdf;base64,\(pdf.base64EncodedString())"
        )
        let attributes = try FileManager.default.attributesOfItem(atPath: body.url.path)
        #expect((attributes[.posixPermissions] as? NSNumber)?.intValue == 0o600)
    }

    @Test("rejects a request that would exceed the encoded body budget")
    func rejectsOversizedEncodedBody() throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let sourceURL = fixture.root.appendingPathComponent("large.bin")
        #expect(FileManager.default.createFile(atPath: sourceURL.path, contents: nil))
        let handle = try FileHandle(forWritingTo: sourceURL)
        try handle.truncate(atOffset: UInt64(AgentModelRequestBodyFileWriter.maximumSourceBytes))
        try handle.close()

        #expect(throws: AgentRuntimeError.self) {
            _ = try AgentModelRequestBodyFileWriter.write(
                payload: [
                    "first": "placeholder-1",
                    "second": "placeholder-2",
                    "third": "placeholder-3",
                ],
                sources: [
                    .init(placeholder: "placeholder-1", url: sourceURL, mimeType: "image/png"),
                    .init(placeholder: "placeholder-2", url: sourceURL, mimeType: "image/png"),
                    .init(placeholder: "placeholder-3", url: sourceURL, mimeType: "image/png"),
                ],
                temporaryDirectory: fixture.root,
                availableCapacityOverride: Int64.max
            )
        }
        let artifactDirectory = fixture.root.appendingPathComponent(
            ".chatos-agent-model-requests",
            isDirectory: true
        )
        if FileManager.default.fileExists(atPath: artifactDirectory.path) {
            let requestFiles = try FileManager.default.contentsOfDirectory(
                at: artifactDirectory,
                includingPropertiesForKeys: nil
            )
            #expect(requestFiles.isEmpty)
        }
    }

    @Test("cleanup removes only stale controlled request files")
    func cleanupRemovesOnlyStaleControlledFiles() throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let sourceURL = fixture.root.appendingPathComponent("source.bin")
        try Data("source".utf8).write(to: sourceURL)
        let body = try AgentModelRequestBodyFileWriter.write(
            payload: ["image": "placeholder"],
            sources: [.init(
                placeholder: "placeholder",
                url: sourceURL,
                mimeType: "image/png"
            )],
            temporaryDirectory: fixture.root,
            availableCapacityOverride: Int64.max
        )
        let unknown = body.url.deletingLastPathComponent().appendingPathComponent("keep.txt")
        try Data("keep".utf8).write(to: unknown)
        let future = Date().addingTimeInterval(25 * 60 * 60)

        #expect(AgentModelRequestBodyFileWriter.reconcileOrphans(
            in: fixture.root,
            olderThan: future
        ) == 1)
        #expect(!FileManager.default.fileExists(atPath: body.url.path))
        #expect(FileManager.default.fileExists(atPath: unknown.path))
    }

    @Test("rejects symbolic link sources")
    func rejectsSymbolicLinkSource() throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let sourceURL = fixture.root.appendingPathComponent("source.bin")
        let linkURL = fixture.root.appendingPathComponent("source-link.bin")
        try Data("source".utf8).write(to: sourceURL)
        try FileManager.default.createSymbolicLink(
            at: linkURL,
            withDestinationURL: sourceURL
        )

        #expect(throws: AgentRuntimeError.self) {
            _ = try AgentModelRequestBodyFileWriter.write(
                payload: ["image": "placeholder"],
                sources: [.init(
                    placeholder: "placeholder",
                    url: linkURL,
                    mimeType: "image/png"
                )],
                temporaryDirectory: fixture.root,
                availableCapacityOverride: Int64.max
            )
        }
    }

    private struct Fixture {
        let root: URL
    }

    private func makeFixture() throws -> Fixture {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "agent-model-request-writer-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return .init(root: root)
    }
}
