@testable import ChatOSConnector
import Foundation
import Testing

@Suite("Native Bounded File Reader")
struct NativeBoundedFileReaderTests {
    @Test("reads a regular file below the configured limit")
    func readsBoundedFile() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("bounded-file-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        let expected = Data("bounded".utf8)
        try expected.write(to: url)

        #expect(try NativeBoundedFileReader.read(url, maximumBytes: 64) == expected)
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

        #expect(throws: NativeBoundedFileReadError.self) {
            _ = try NativeBoundedFileReader.read(url, maximumBytes: 1_024)
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
            return try NativeBoundedFileReader.read(url, maximumBytes: 1_024 * 1_024)
        }

        await #expect(throws: CancellationError.self) {
            _ = try await task.value
        }
    }

    @Test("connector state refuses an oversized file")
    func connectorStateRejectsOversizedFile() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("connector-state-large-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent("state.json")
        #expect(FileManager.default.createFile(atPath: url.path, contents: nil))
        let handle = try FileHandle(forWritingTo: url)
        try handle.truncate(atOffset: UInt64(NativeConnectorStateStore.maximumStateBytes + 1))
        try handle.close()

        #expect(throws: NativeBoundedFileReadError.self) {
            _ = try NativeConnectorStateStore(stateURL: url).load()
        }
    }
}
