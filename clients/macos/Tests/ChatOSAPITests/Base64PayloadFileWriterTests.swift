import Foundation
import XCTest
@testable import ChatOSAPI

final class Base64PayloadFileWriterTests: XCTestCase {
    func testDecodedImageUsesBoundedLeaseOwnedArtifact() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "Base64PayloadFileWriterTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let expected = Data((0..<400_000).map { UInt8($0 % 251) })

        let file = try await ChatOSBase64PayloadFileWriter.write(
            expected.base64EncodedString(),
            maximumBytes: expected.count,
            temporaryDirectory: root
        )

        let artifact = file.url.deletingLastPathComponent()
        XCTAssertTrue(artifact.lastPathComponent.hasPrefix(".chatos-image-response-"))
        XCTAssertEqual(file.byteCount, expected.count)
        XCTAssertEqual(try Data(contentsOf: file.url), expected)
        XCTAssertEqual(try permissions(artifact), 0o700)
        XCTAssertEqual(try permissions(file.url), 0o600)
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
                artifactKind: .imageResponse
            ),
            0
        )
        XCTAssertTrue(FileManager.default.fileExists(atPath: file.url.path))

        file.removeIfNeeded()
        XCTAssertFalse(FileManager.default.fileExists(atPath: artifact.path))
    }

    private func permissions(_ url: URL) throws -> Int {
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        return try XCTUnwrap(attributes[.posixPermissions] as? NSNumber).intValue
    }
}
