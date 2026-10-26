import Darwin
import Foundation
import XCTest
@testable import ChatOSConnector

final class NativeLocalTemporaryFileMaintenanceServiceTests: XCTestCase {
    func testCleanupDeletesOnlyOldUnlockedOwnedTemporaryFiles() async throws {
        let fixture = try TemporaryMaintenanceFixture()
        defer { fixture.dispose() }
        let staleVault = try fixture.makeFile(
            relativePath: "LocalAgent/Attachments/owner/conversation/" + temporaryWriteName(),
            stale: true
        )
        let staleStaging = try fixture.makeFile(
            relativePath: "LocalAgent/local-agent.artifact-staging/" + temporaryArtifactName(),
            stale: true
        )
        let staleRestore = try fixture.makeFile(
            relativePath: "AgentGroupChatAttachments/message/" + temporaryArtifactName(),
            stale: true
        )
        let fresh = try fixture.makeFile(
            relativePath: "AgentGroupChatAttachments/message/" + temporaryArtifactName(),
            stale: false
        )
        let unknown = try fixture.makeFile(
            relativePath: "AgentGroupChatAttachments/message/.chatos-artifact-copy-not-a-uuid.tmp",
            stale: true
        )
        let locked = try fixture.makeFile(
            relativePath: "LocalAgent/local-agent.artifact-staging/" + temporaryArtifactName(),
            stale: true
        )
        let lockedDescriptor = locked.path.withCString {
            Darwin.open($0, O_RDWR | O_NOFOLLOW | O_CLOEXEC)
        }
        XCTAssertGreaterThanOrEqual(lockedDescriptor, 0)
        defer { if lockedDescriptor >= 0 { _ = Darwin.close(lockedDescriptor) } }
        XCTAssertEqual(flock(lockedDescriptor, LOCK_EX | LOCK_NB), 0)

        let report = try await NativeLocalTemporaryFileMaintenanceService(
            rootURL: fixture.root,
            codeReadWorker: ConnectorCoreCodeReadWorker()
        ).cleanupStaleFiles()

        XCTAssertEqual(report.eligibleEntries, 4)
        XCTAssertEqual(report.deletedEntries, 3)
        XCTAssertEqual(report.lockedEntries, 1)
        XCTAssertFalse(report.truncated)
        XCTAssertFalse(FileManager.default.fileExists(atPath: staleVault.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: staleStaging.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: staleRestore.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: fresh.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: unknown.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: locked.path))
    }

    func testCleanupRejectsShortAgeWindow() async throws {
        let fixture = try TemporaryMaintenanceFixture()
        defer { fixture.dispose() }
        do {
            _ = try await NativeLocalTemporaryFileMaintenanceService(
                rootURL: fixture.root,
                codeReadWorker: ConnectorCoreCodeReadWorker()
            ).cleanupStaleFiles(minimumAgeSeconds: 60)
            XCTFail("unsafe cleanup age was accepted")
        } catch NativeLocalTemporaryFileMaintenanceError.invalidConfiguration {}
    }
}

private struct TemporaryMaintenanceFixture {
    let root: URL

    init() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "temporary-maintenance-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    func makeFile(relativePath: String, stale: Bool) throws -> URL {
        let file = root.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(
            at: file.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("temporary".utf8).write(to: file)
        if stale {
            try FileManager.default.setAttributes(
                [.modificationDate: Date(timeIntervalSinceNow: -(25 * 60 * 60))],
                ofItemAtPath: file.path
            )
        }
        return file
    }

    func dispose() { try? FileManager.default.removeItem(at: root) }
}

private func temporaryWriteName() -> String {
    ".chatos-write-\(UUID().uuidString.lowercased()).tmp"
}

private func temporaryArtifactName() -> String {
    ".chatos-artifact-copy-\(UUID().uuidString.lowercased()).tmp"
}
