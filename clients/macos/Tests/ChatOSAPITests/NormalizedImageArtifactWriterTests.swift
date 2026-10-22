import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class NormalizedImageArtifactWriterTests: XCTestCase {
    func testNormalizedImageWriterCreatesLeaseOwnedBoundedArtifact() async throws {
        let root = try temporaryDirectory()
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let expected = Data((0..<64_000).map { UInt8($0 % 251) })
        var file: GeneratedMediaFile? = try await ChatOSMediaArtifactFileWriter
            .writeNormalizedImage(
                maximumBytes: expected.count,
                temporaryDirectory: root,
                availableCapacityOverride: Int64.max
            ) { destinationHandle in
                try destinationHandle.write(contentsOf: expected)
            }

        let artifact = try XCTUnwrap(file).url.deletingLastPathComponent()
        XCTAssertTrue(artifact.lastPathComponent.hasPrefix(".chatos-image-normalized-"))
        XCTAssertEqual(file?.byteCount, expected.count)
        XCTAssertEqual(try Data(contentsOf: XCTUnwrap(file).url), expected)
        XCTAssertEqual(try permissions(artifact), 0o700)
        XCTAssertEqual(try permissions(artifact.appendingPathComponent(".lease")), 0o600)
        XCTAssertEqual(try permissions(XCTUnwrap(file).url), 0o600)
        XCTAssertEqual(
            Set(try FileManager.default.contentsOfDirectory(atPath: artifact.path)),
            [".lease", "payload.tmp"]
        )

        try FileManager.default.setAttributes(
            [.modificationDate: Date(timeIntervalSince1970: 0)],
            ofItemAtPath: artifact.path
        )
        XCTAssertEqual(
            ChatOSResponseBodyFileWriter.reconcileOrphanedArtifacts(
                in: root,
                olderThan: Date(),
                artifactKind: .normalizedImage
            ),
            0
        )

        file = nil
        XCTAssertFalse(FileManager.default.fileExists(atPath: artifact.path))
    }

    func testNormalizedImageWriterRejectsOversizedAndLowDiskWritesWithoutArtifacts() async throws {
        let root = try temporaryDirectory()
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }

        do {
            _ = try await ChatOSMediaArtifactFileWriter.writeNormalizedImage(
                maximumBytes: 4,
                temporaryDirectory: root,
                availableCapacityOverride: Int64.max
            ) { destinationHandle in
                try destinationHandle.write(contentsOf: Data("oversized".utf8))
            }
            XCTFail("Oversized normalized image unexpectedly succeeded")
        } catch MediaGenerationClientError.invalidVideoContent {
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), [])
        }

        do {
            _ = try await ChatOSMediaArtifactFileWriter.writeNormalizedImage(
                maximumBytes: 64,
                temporaryDirectory: root,
                availableCapacityOverride: 0
            ) { destinationHandle in
                try destinationHandle.write(contentsOf: Data("image".utf8))
            }
            XCTFail("Low disk capacity unexpectedly admitted a normalized image")
        } catch MediaGenerationClientError.insufficientDiskSpace {
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), [])
        }
    }

    private func temporaryDirectory() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "NormalizedImageArtifactWriterTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    private func permissions(_ url: URL) throws -> Int {
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        return try XCTUnwrap(attributes[.posixPermissions] as? NSNumber).intValue
    }
}
