@testable import ChatOSConnector
import Foundation
import Testing

@Suite("Connector Bounded File Integration")
struct NativeBoundedFileReaderTests {
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
