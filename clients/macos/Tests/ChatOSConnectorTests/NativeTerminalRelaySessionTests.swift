@testable import ChatOSConnector
import Foundation
import XCTest

final class NativeTerminalRelaySessionTests: XCTestCase {
    func testOutputJournalUsesMonotonicSequenceAndReportsTruncation() {
        let journal = NativeTerminalRelayOutputJournal(maximumBytes: 8)

        XCTAssertEqual(journal.append("1234"), 1)
        XCTAssertEqual(journal.append("5678"), 2)
        XCTAssertEqual(journal.append("90"), 3)

        let snapshot = journal.snapshot(maximumLines: 500)
        XCTAssertEqual(snapshot.data, "567890")
        XCTAssertEqual(snapshot.baseSequence, 2)
        XCTAssertEqual(snapshot.sequence, 3)
        XCTAssertTrue(snapshot.truncated)
    }

    func testOutputJournalLimitsSnapshotLines() {
        let journal = NativeTerminalRelayOutputJournal(maximumBytes: 1_024)
        _ = journal.append("one\ntwo\nthree\n")

        let snapshot = journal.snapshot(maximumLines: 2)

        XCTAssertEqual(snapshot.data, "three\n")
        XCTAssertTrue(snapshot.truncated)
    }

    func testSnapshotTransportPayloadIsBoundedBelowRelayEventLimit() {
        let journal = NativeTerminalRelayOutputJournal(maximumBytes: 64 * 1_024)
        _ = journal.append(String(repeating: "x", count: 32 * 1_024))

        let snapshot = journal.snapshot(maximumLines: 500)

        XCTAssertLessThanOrEqual(snapshot.data.utf8.count, 16 * 1_024)
        XCTAssertTrue(snapshot.truncated)
    }

    func testLocalRelaySessionStreamsThroughAPersistentPTY() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-terminal-relay-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }

        let session = NativeLocalTerminalRelaySession(
            terminalSessionID: "terminal-test",
            workspaceID: "workspace-test",
            workingDirectory: directory.path,
            columns: 80,
            rows: 24
        )
        try session.start()
        defer { session.close() }

        try session.send(Data("printf '__CHATOS_PTY_READY__\\n'\n".utf8))
        let deadline = Date().addingTimeInterval(3)
        var snapshot = session.snapshot(maximumLines: 500)
        while !snapshot.data.contains("__CHATOS_PTY_READY__"), Date() < deadline {
            try await Task.sleep(for: .milliseconds(25))
            snapshot = session.snapshot(maximumLines: 500)
        }

        XCTAssertTrue(session.running)
        XCTAssertTrue(snapshot.data.contains("__CHATOS_PTY_READY__"))
        XCTAssertGreaterThan(snapshot.sequence, 0)
    }

    func testLocalRelaySessionRejectsOversizedInputFrames() throws {
        let session = NativeLocalTerminalRelaySession(
            terminalSessionID: "terminal-test",
            workspaceID: "workspace-test",
            workingDirectory: FileManager.default.temporaryDirectory.path,
            columns: 80,
            rows: 24
        )

        XCTAssertThrowsError(try session.send(Data(count: 64 * 1_024 + 1)))
        session.close()
    }
}
