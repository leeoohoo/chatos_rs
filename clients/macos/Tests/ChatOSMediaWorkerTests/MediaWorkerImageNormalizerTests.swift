import ChatOSMediaWorkerCore
import ChatOSMediaWorkerProtocol
import Darwin
import Foundation
import XCTest

final class MediaWorkerImageNormalizerTests: XCTestCase {
    func testNormalizePNGUsesAuthorizedPrecreatedDestination() throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        let descriptor = try ChatOSMediaWorkerEngine.normalizePNG(
            try request(source: fixture.source, destination: fixture.destination)
        )

        XCTAssertEqual(descriptor.path, fixture.destination.path)
        XCTAssertEqual(descriptor.mimeType, "image/png")
        XCTAssertGreaterThan(descriptor.byteCount, 0)
        XCTAssertEqual(descriptor.sha256.count, 64)
        XCTAssertEqual(descriptor.pixelWidth, 1)
        XCTAssertEqual(descriptor.pixelHeight, 1)
        XCTAssertEqual(try Data(contentsOf: fixture.destination).count, descriptor.byteCount)
    }

    func testNormalizePNGAcceptsProtocolByteCeiling() throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        let descriptor = try ChatOSMediaWorkerEngine.normalizePNG(try request(
            source: fixture.source,
            destination: fixture.destination,
            maximumSourceBytes: ChatOSMediaWorkerProtocol.maximumNormalizedImageBytes,
            maximumOutputBytes: ChatOSMediaWorkerProtocol.maximumNormalizedImageBytes
        ))

        XCTAssertEqual(descriptor.pixelWidth, 1)
        XCTAssertEqual(descriptor.pixelHeight, 1)
    }

    func testNormalizePNGRejectsByteLimitAboveProtocolCeiling() throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        XCTAssertThrowsError(try ChatOSMediaWorkerEngine.normalizePNG(try request(
            source: fixture.source,
            destination: fixture.destination,
            maximumSourceBytes: ChatOSMediaWorkerProtocol.maximumNormalizedImageBytes + 1
        ))) { error in
            guard case MediaWorkerCoreError.invalidRequest = error else {
                return XCTFail("Unexpected error: \(error)")
            }
        }
    }

    func testNormalizePNGRejectsSymlinkSource() throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let link = fixture.root.appendingPathComponent("source-link.png")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: fixture.source)
        let sourceIdentity = try identity(fixture.source)

        XCTAssertThrowsError(try ChatOSMediaWorkerEngine.normalizePNG(
            try request(
                source: link,
                destination: fixture.destination,
                sourceIdentity: sourceIdentity
            )
        )) { error in
            guard case MediaWorkerCoreError.unauthorizedPath = error else {
                return XCTFail("Unexpected error: \(error)")
            }
        }
    }

    func testNormalizePNGRejectsReplacedDestinationIdentity() throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let originalIdentity = try identity(fixture.destination)
        try FileManager.default.removeItem(at: fixture.destination)
        XCTAssertTrue(FileManager.default.createFile(
            atPath: fixture.destination.path,
            contents: nil,
            attributes: [.posixPermissions: 0o600]
        ))

        XCTAssertThrowsError(try ChatOSMediaWorkerEngine.normalizePNG(
            try request(
                source: fixture.source,
                destination: fixture.destination,
                destinationIdentity: originalIdentity
            )
        )) { error in
            guard case MediaWorkerCoreError.unauthorizedPath = error else {
                return XCTFail("Unexpected error: \(error)")
            }
        }
    }

    func testNormalizePNGRejectsExpiredDeadline() throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        XCTAssertThrowsError(try ChatOSMediaWorkerEngine.normalizePNG(
            try request(
                source: fixture.source,
                destination: fixture.destination,
                deadline: 1
            )
        )) { error in
            XCTAssertEqual(error as? MediaWorkerCoreError, .deadlineExceeded)
        }
    }

    func testVideoFrameRejectsSymlinkSource() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let link = fixture.root.appendingPathComponent("source-link.mov")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: fixture.source)
        do {
            _ = try await ChatOSMediaWorkerEngine.extractLastVideoFramePNG(
                try videoRequest(
                    source: link,
                    destination: fixture.destination,
                    sourceIdentity: identity(fixture.source)
                )
            )
            XCTFail("A symlink video source was accepted")
        } catch MediaWorkerCoreError.unauthorizedPath {
            // Expected before AVFoundation receives the path.
        }
    }

    func testVideoFrameRejectsExpiredDeadline() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        do {
            _ = try await ChatOSMediaWorkerEngine.extractLastVideoFramePNG(
                try videoRequest(
                    source: fixture.source,
                    destination: fixture.destination,
                    deadline: 1
                )
            )
            XCTFail("An expired video-frame request was accepted")
        } catch let error as MediaWorkerCoreError {
            XCTAssertEqual(error, .deadlineExceeded)
        }
    }

    private func makeFixture() throws -> (root: URL, source: URL, destination: URL) {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "ChatOSMediaWorkerTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(
            at: root,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        let source = root.appendingPathComponent("source.png")
        let destination = root.appendingPathComponent("destination.png")
        let png = try XCTUnwrap(Data(base64Encoded:
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
        ))
        try png.write(to: source, options: .withoutOverwriting)
        XCTAssertTrue(FileManager.default.createFile(
            atPath: destination.path,
            contents: nil,
            attributes: [.posixPermissions: 0o600]
        ))
        return (root, source, destination)
    }

    private func request(
        source: URL,
        destination: URL,
        sourceIdentity: MediaWorkerFileIdentity? = nil,
        destinationIdentity: MediaWorkerFileIdentity? = nil,
        deadline: Int64? = nil,
        maximumSourceBytes: Int = 20 * 1_024 * 1_024,
        maximumOutputBytes: Int = 20 * 1_024 * 1_024
    ) throws -> MediaWorkerNormalizePNGRequest {
        .init(
            protocolVersion: ChatOSMediaWorkerProtocol.maximumVersion,
            requestID: UUID().uuidString.lowercased(),
            ownerUserID: "worker-test",
            accountSessionID: "worker-test-session",
            sessionEpoch: "worker-test-epoch",
            helperInstanceID: "worker-test-instance",
            deadlineUnixMilliseconds: deadline
                ?? Int64((Date().timeIntervalSince1970 + 30) * 1_000),
            sourcePath: source.path,
            sourceIdentity: try sourceIdentity ?? identity(source),
            destinationPath: destination.path,
            destinationIdentity: try destinationIdentity ?? identity(destination),
            maximumSourceBytes: maximumSourceBytes,
            maximumSourcePixelCount: 64_000_000,
            maximumOutputPixelSize: 8_192,
            maximumOutputBytes: maximumOutputBytes
        )
    }

    private func videoRequest(
        source: URL,
        destination: URL,
        sourceIdentity: MediaWorkerFileIdentity? = nil,
        deadline: Int64? = nil
    ) throws -> MediaWorkerExtractVideoFrameRequest {
        .init(
            protocolVersion: ChatOSMediaWorkerProtocol.maximumVersion,
            requestID: UUID().uuidString.lowercased(),
            ownerUserID: "worker-test",
            accountSessionID: "worker-test-session",
            sessionEpoch: "worker-test-epoch",
            helperInstanceID: "worker-test-instance",
            deadlineUnixMilliseconds: deadline
                ?? Int64((Date().timeIntervalSince1970 + 30) * 1_000),
            sourcePath: source.path,
            sourceIdentity: try sourceIdentity ?? identity(source),
            destinationPath: destination.path,
            destinationIdentity: try identity(destination),
            maximumSourceBytes: 512 * 1_024 * 1_024,
            maximumOutputPixelSize: 4_096,
            maximumOutputBytes: 20 * 1_024 * 1_024
        )
    }

    private func identity(_ url: URL) throws -> MediaWorkerFileIdentity {
        var status = stat()
        guard url.path.withCString({ Darwin.lstat($0, &status) }) == 0 else {
            throw CocoaError(.fileReadNoSuchFile)
        }
        return .init(
            device: UInt64(status.st_dev),
            inode: UInt64(status.st_ino),
            byteCount: Int64(status.st_size)
        )
    }
}
