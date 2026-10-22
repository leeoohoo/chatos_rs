@testable import ChatOSAPI
import ChatOSCore
import Darwin
import Foundation
import XCTest

final class UploadBodyArtifactTests: XCTestCase {
    func testVideoAndImageWritersUsePrivateLeaseAwareArtifacts() throws {
        let root = try makeRoot("upload-artifact-writers")
        defer { try? FileManager.default.removeItem(at: root) }
        let videoSource = root.appendingPathComponent("source.mp4")
        try Data("video-source".utf8).write(to: videoSource)
        let videoBody = try ChatOSVideoRequestBodyFileWriter.write(
            payload: ["input": "__VIDEO__"],
            placeholder: "__VIDEO__",
            sourceURL: videoSource,
            mimeType: "video/mp4",
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max
        )
        try assertArtifact(
            videoBody,
            prefix: ".chatos-video-request-",
            payloadName: "request.json"
        )
        let videoDirectory = videoBody.url.deletingLastPathComponent()

        let imageSource = root.appendingPathComponent("source.png")
        try Data("image-source".utf8).write(to: imageSource)
        let imageBody = try ChatOSImageMultipartBodyFileWriter.write(
            runtime: .init(
                provider: "openai",
                model: "gpt-image-1",
                apiKey: "secret",
                baseURL: "https://provider.example/v1",
                enabled: true
            ),
            request: .init(
                modelConfigID: "image",
                prompt: "edit",
                size: nil,
                count: 1,
                inputImage: .init(
                    name: "source.png", mimeType: "image/png", fileURL: imageSource
                )
            ),
            images: [
                .init(name: "source.png", mimeType: "image/png", fileURL: imageSource),
            ],
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max
        ).bodyFile
        try assertArtifact(
            imageBody,
            prefix: ".chatos-image-request-",
            payloadName: "request.multipart"
        )
        let imageDirectory = imageBody.url.deletingLastPathComponent()

        videoBody.removeIfNeeded()
        imageBody.removeIfNeeded()
        XCTAssertFalse(FileManager.default.fileExists(atPath: videoDirectory.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: imageDirectory.path))
    }

    func testActiveUploadLeaseProtectsExpiredRequest() throws {
        let root = try makeRoot("upload-artifact-active")
        defer { try? FileManager.default.removeItem(at: root) }
        let source = root.appendingPathComponent("source.mp4")
        try Data("video-source".utf8).write(to: source)
        let body = try ChatOSVideoRequestBodyFileWriter.write(
            payload: ["input": "__VIDEO__"],
            placeholder: "__VIDEO__",
            sourceURL: source,
            mimeType: "video/mp4",
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max
        )
        let artifact = body.url.deletingLastPathComponent()
        try expire(artifact)

        XCTAssertEqual(
            ChatOSUploadBodyArtifact.reconcileOrphans(
                in: root, olderThan: Date(), maximumCandidates: 64
            ),
            0
        )
        XCTAssertTrue(FileManager.default.fileExists(atPath: body.url.path))
        body.removeIfNeeded()
    }

    func testUploadReconciliationIsBoundedAndPreservesMalformedEntries() throws {
        let root = try makeRoot("upload-artifact-maintenance")
        defer { try? FileManager.default.removeItem(at: root) }
        for index in 0..<65 {
            let kind: ChatOSUploadBodyArtifact.Kind = index.isMultiple(of: 2)
                ? .videoJSON : .imageMultipart
            try makeOrphan(kind: kind, in: root)
        }
        let malformed = root.appendingPathComponent(
            ".chatos-video-request-not-a-uuid", isDirectory: true
        )
        try FileManager.default.createDirectory(at: malformed, withIntermediateDirectories: false)
        try expire(malformed)

        XCTAssertEqual(
            ChatOSUploadBodyArtifact.reconcileOrphans(
                in: root, olderThan: Date(), maximumCandidates: 64
            ),
            64
        )
        XCTAssertEqual(try artifactDirectories(in: root).count, 2)
        XCTAssertEqual(
            ChatOSUploadBodyArtifact.reconcileOrphans(
                in: root, olderThan: Date(), maximumCandidates: 64
            ),
            1
        )
        let remaining = try artifactDirectories(in: root)
        XCTAssertEqual(remaining.count, 1)
        XCTAssertEqual(remaining.first?.lastPathComponent, malformed.lastPathComponent)
    }

    func testUploadReconciliationPreservesUnsafeShapes() throws {
        let root = try makeRoot("upload-artifact-safety")
        defer { try? FileManager.default.removeItem(at: root) }

        let symlinkTarget = try makeRoot("upload-artifact-symlink-target")
        defer { try? FileManager.default.removeItem(at: symlinkTarget) }
        let symlinkDirectory = artifactDirectory(kind: .videoJSON, in: root)
        try FileManager.default.createSymbolicLink(
            at: symlinkDirectory, withDestinationURL: symlinkTarget
        )

        let symlinkPayload = try makeUnsafeArtifact(kind: .videoJSON, in: root)
        let payloadTarget = root.appendingPathComponent("payload-target")
        try Data("target".utf8).write(to: payloadTarget)
        try FileManager.default.createSymbolicLink(
            at: symlinkPayload.appendingPathComponent("request.json"),
            withDestinationURL: payloadTarget
        )
        try expire(symlinkPayload)

        let hardLinkedPayload = try makeUnsafeArtifact(kind: .imageMultipart, in: root)
        let hardLinkTarget = root.appendingPathComponent("hard-link-target")
        try Data("target".utf8).write(to: hardLinkTarget)
        try FileManager.default.linkItem(
            at: hardLinkTarget,
            to: hardLinkedPayload.appendingPathComponent("request.multipart")
        )
        try expire(hardLinkedPayload)

        let unknownFile = try makeUnsafeArtifact(
            kind: .videoJSON, in: root, createPayload: true
        )
        try Data("unknown".utf8).write(
            to: unknownFile.appendingPathComponent("unexpected")
        )
        try expire(unknownFile)

        let leaseLess = artifactDirectory(kind: .imageMultipart, in: root)
        try FileManager.default.createDirectory(at: leaseLess, withIntermediateDirectories: false)
        try Data("legacy".utf8).write(
            to: leaseLess.appendingPathComponent("request.multipart")
        )
        try expire(leaseLess)

        XCTAssertEqual(
            ChatOSUploadBodyArtifact.reconcileOrphans(
                in: root, olderThan: Date(), maximumCandidates: 64
            ),
            0
        )
        for preserved in [
            symlinkDirectory, symlinkPayload, hardLinkedPayload, unknownFile, leaseLess,
        ] {
            XCTAssertTrue(
                FileManager.default.fileExists(atPath: preserved.path),
                "Expected \(preserved.lastPathComponent) to be preserved"
            )
        }
    }

    private func assertArtifact(
        _ body: HTTPRequestBodyFile,
        prefix: String,
        payloadName: String
    ) throws {
        let directory = body.url.deletingLastPathComponent()
        XCTAssertTrue(directory.lastPathComponent.hasPrefix(prefix))
        XCTAssertEqual(body.url.lastPathComponent, payloadName)
        XCTAssertGreaterThan(body.byteCount, 0)
        XCTAssertEqual(
            Set(try FileManager.default.contentsOfDirectory(atPath: directory.path)),
            [".lease", payloadName]
        )
        XCTAssertEqual(fileMode(directory), 0o700)
        XCTAssertEqual(fileMode(directory.appendingPathComponent(".lease")), 0o600)
        XCTAssertEqual(fileMode(body.url), 0o600)
    }

    private func makeRoot(_ prefix: String) throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "\(prefix)-\(UUID().uuidString)", isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        return root
    }

    private func makeOrphan(
        kind: ChatOSUploadBodyArtifact.Kind,
        in root: URL
    ) throws {
        let directory = artifactDirectory(kind: kind, in: root)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        XCTAssertTrue(FileManager.default.createFile(
            atPath: directory.appendingPathComponent(".lease").path,
            contents: Data()
        ))
        XCTAssertTrue(FileManager.default.createFile(
            atPath: directory.appendingPathComponent(kind.payloadFilename).path,
            contents: Data("body".utf8)
        ))
        try expire(directory)
    }

    private func artifactDirectory(
        kind: ChatOSUploadBodyArtifact.Kind,
        in root: URL
    ) -> URL {
        root.appendingPathComponent(
            "\(kind.prefix)\(UUID().uuidString.lowercased())", isDirectory: true
        )
    }

    private func makeUnsafeArtifact(
        kind: ChatOSUploadBodyArtifact.Kind,
        in root: URL,
        createPayload: Bool = false
    ) throws -> URL {
        let directory = artifactDirectory(kind: kind, in: root)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        XCTAssertTrue(FileManager.default.createFile(
            atPath: directory.appendingPathComponent(".lease").path,
            contents: Data()
        ))
        if createPayload {
            XCTAssertTrue(FileManager.default.createFile(
                atPath: directory.appendingPathComponent(kind.payloadFilename).path,
                contents: Data("body".utf8)
            ))
        }
        return directory
    }

    private func expire(_ url: URL) throws {
        try FileManager.default.setAttributes(
            [.modificationDate: Date().addingTimeInterval(-48 * 60 * 60)],
            ofItemAtPath: url.path
        )
    }

    private func artifactDirectories(in root: URL) throws -> [URL] {
        try FileManager.default.contentsOfDirectory(
            at: root, includingPropertiesForKeys: nil
        ).filter {
            $0.lastPathComponent.hasPrefix(".chatos-video-request-")
                || $0.lastPathComponent.hasPrefix(".chatos-image-request-")
        }
    }

    private func fileMode(_ url: URL) -> mode_t? {
        var status = stat()
        let result = url.withUnsafeFileSystemRepresentation { path in
            path.map { Darwin.lstat($0, &status) } ?? -1
        }
        return result == 0 ? status.st_mode & mode_t(0o777) : nil
    }
}
