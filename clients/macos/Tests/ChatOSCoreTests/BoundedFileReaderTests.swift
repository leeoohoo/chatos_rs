import Foundation
import Testing
@testable import ChatOSCore

@Suite("Bounded File Reader")
struct BoundedFileReaderTests {
    @Test("reads a regular file below the configured limit")
    func readsBoundedFile() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("bounded-file-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        let expected = Data("bounded".utf8)
        try expected.write(to: url)

        #expect(try BoundedFileReader.read(url, maximumBytes: 64) == expected)
    }

    @Test("rejects an oversized sparse file without loading its contents")
    func rejectsOversizedFile() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("bounded-file-large-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        #expect(FileManager.default.createFile(atPath: url.path, contents: nil))
        let handle = try FileHandle(forWritingTo: url)
        try handle.truncate(atOffset: 1_025)
        try handle.close()

        #expect(throws: BoundedFileReadError.self) {
            _ = try BoundedFileReader.read(url, maximumBytes: 1_024)
        }
    }

    @Test("rejects symbolic links")
    func rejectsSymbolicLink() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bounded-file-link-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let target = root.appendingPathComponent("target")
        let link = root.appendingPathComponent("link")
        try Data("bounded".utf8).write(to: target)
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: target)

        #expect(throws: BoundedFileReadError.self) {
            _ = try BoundedFileReader.read(link, maximumBytes: 64)
        }
    }

    @Test("stops reading when the calling task is cancelled")
    func respectsCancellation() async throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("bounded-file-cancelled-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        try Data(repeating: 7, count: 512 * 1_024).write(to: url)

        let task = Task.detached { () throws -> Data in
            withUnsafeCurrentTask { $0?.cancel() }
            return try BoundedFileReader.read(url, maximumBytes: 1_024 * 1_024)
        }

        await #expect(throws: CancellationError.self) {
            _ = try await task.value
        }
    }
}
