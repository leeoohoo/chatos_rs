import Darwin
import Foundation
import XCTest
@testable import ChatOSApp

final class AppOwnedExportTransactionTests: XCTestCase {
    func testCleanupReclaimsOnlyCanonicalStaleUnlockedSingleLinkFiles() throws {
        for prefix in [".chatos-artifact-export-", ".chatos-agent-export-"] {
            let root = try makeDirectory()
            defer { try? FileManager.default.removeItem(at: root) }
            let stale = temporaryURL(in: root, prefix: prefix)
            let fresh = temporaryURL(in: root, prefix: prefix)
            let malformed = root.appendingPathComponent("\(prefix)not-a-uuid.tmp")
            let uppercase = root.appendingPathComponent(
                prefix + UUID().uuidString.uppercased() + ".tmp"
            )
            let locked = temporaryURL(in: root, prefix: prefix)
            let hardLink = temporaryURL(in: root, prefix: prefix)
            let symlink = temporaryURL(in: root, prefix: prefix)
            let hardLinkSource = root.appendingPathComponent("hard-link-source")
            let symlinkSource = root.appendingPathComponent("symlink-source")

            for url in [stale, fresh, malformed, uppercase, locked, hardLinkSource, symlinkSource] {
                try Data("temporary".utf8).write(to: url)
            }
            try FileManager.default.linkItem(at: hardLinkSource, to: hardLink)
            try FileManager.default.createSymbolicLink(at: symlink, withDestinationURL: symlinkSource)
            for url in [stale, malformed, uppercase, locked, hardLink] {
                try makeStale(url)
            }
            let lockedDescriptor = locked.path.withCString {
                Darwin.open($0, O_RDWR | O_NOFOLLOW | O_CLOEXEC)
            }
            XCTAssertGreaterThanOrEqual(lockedDescriptor, 0)
            defer { if lockedDescriptor >= 0 { _ = Darwin.close(lockedDescriptor) } }
            XCTAssertEqual(flock(lockedDescriptor, LOCK_EX | LOCK_NB), 0)

            let transaction = try AppOwnedExportTransaction(
                destinationURL: root.appendingPathComponent("result.bin"),
                temporaryPrefix: prefix
            )

            XCTAssertFalse(FileManager.default.fileExists(atPath: stale.path))
            for preserved in [fresh, malformed, uppercase, locked, hardLink, symlink] {
                XCTAssertTrue(
                    FileManager.default.fileExists(atPath: preserved.path),
                    "Unexpectedly removed \(preserved.lastPathComponent)"
                )
            }
            withExtendedLifetime(transaction) {}
        }
    }

    func testUninstalledTransactionRemovesItsTemporaryFile() throws {
        let root = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        var transaction: AppOwnedExportTransaction? = try .init(
            destinationURL: root.appendingPathComponent("result.bin"),
            temporaryPrefix: ".chatos-artifact-export-"
        )
        XCTAssertNotNil(transaction)
        XCTAssertEqual(try temporaryFiles(in: root).count, 1)

        transaction = nil

        XCTAssertTrue(try temporaryFiles(in: root).isEmpty)
    }

    func testInstallRejectsDirectoryPathReplacementAndCleansOriginalDirectory() throws {
        let root = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let selected = root.appendingPathComponent("selected", isDirectory: true)
        let moved = root.appendingPathComponent("moved", isDirectory: true)
        try FileManager.default.createDirectory(at: selected, withIntermediateDirectories: false)
        let destination = selected.appendingPathComponent("result.bin")
        var transaction: AppOwnedExportTransaction? = try .init(
            destinationURL: destination,
            temporaryPrefix: ".chatos-agent-export-"
        )
        let bytes = Data("verified export".utf8)
        let written = bytes.withUnsafeBytes { buffer in
            Darwin.write(transaction?.fileDescriptor ?? -1, buffer.baseAddress, buffer.count)
        }
        XCTAssertEqual(written, bytes.count)
        try FileManager.default.moveItem(at: selected, to: moved)
        try FileManager.default.createDirectory(at: selected, withIntermediateDirectories: false)

        XCTAssertThrowsError(try transaction?.install(as: destination.lastPathComponent))
        XCTAssertFalse(FileManager.default.fileExists(atPath: destination.path))

        transaction = nil
        XCTAssertTrue(try temporaryFiles(in: moved).isEmpty)
    }

    private func makeDirectory() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "AppOwnedExportTransactionTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    private func temporaryURL(in root: URL, prefix: String) -> URL {
        root.appendingPathComponent(prefix + UUID().uuidString.lowercased() + ".tmp")
    }

    private func makeStale(_ url: URL) throws {
        try FileManager.default.setAttributes(
            [.modificationDate: Date(timeIntervalSinceNow: -(25 * 60 * 60))],
            ofItemAtPath: url.path
        )
    }

    private func temporaryFiles(in directory: URL) throws -> [String] {
        try FileManager.default.contentsOfDirectory(atPath: directory.path).filter {
            $0.hasPrefix(".chatos-") && $0.hasSuffix(".tmp")
        }
    }
}
