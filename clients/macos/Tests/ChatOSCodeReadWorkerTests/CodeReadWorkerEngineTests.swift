import ChatOSCodeReadWorkerCore
import ChatOSCodeReadWorkerProtocol
import CryptoKit
import Darwin
import Foundation
import XCTest

final class CodeReadWorkerEngineTests: XCTestCase {
    func testReadsListsAndSearchesWithinAuthorizedRoot() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let sources = fixture.root.appendingPathComponent("Sources", isDirectory: true)
        try FileManager.default.createDirectory(at: sources, withIntermediateDirectories: true)
        try Data("struct Station {\n    let dockingCode = \"NOVA\"\n}\n".utf8)
            .write(to: sources.appendingPathComponent("Station.swift"))

        let listed = try fixture.execute("list_dir", ["path": .string(".")]).object
        XCTAssertTrue(try listed.array("entries").contains {
            try $0.object.string("name") == "Sources"
        })
        let read = try fixture.execute(
            "read_file_raw",
            ["path": .string("Sources/Station.swift")]
        ).object
        XCTAssertTrue(try read.string("content").contains("dockingCode"))
        XCTAssertEqual(try read.string("sha256").count, 64)

        let searched = try fixture.execute(
            "search_text",
            ["pattern": .string("dockingCode")]
        ).object
        XCTAssertEqual(try searched.number("count"), 1)
        XCTAssertEqual(
            try searched.array("results").first?.object.string("path"),
            "Sources/Station.swift"
        )

        let presented = try fixture.execute(
            "open_file_in_pet",
            [
                "path": .string("Sources/Station.swift"),
                "line": .number(2),
                "mode": .string("edit"),
            ]
        ).object
        XCTAssertEqual(try presented.string("relative_path"), "Sources/Station.swift")
        XCTAssertEqual(try presented.number("line"), 2)
        XCTAssertEqual(try presented.string("mode"), "edit")
    }

    func testVerifiesFileIdentityAndRejectsDigestMismatchAndSymlink() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let data = Data(repeating: 0x7a, count: 96 * 1_024)
        let file = fixture.root.appendingPathComponent("preview.png")
        try data.write(to: file)
        let arguments: [String: CodeReadWorkerJSONValue] = [
            "name": .string(file.lastPathComponent),
            "byte_size": .number(Double(data.count)),
            "sha256": .string(sha256(data)),
        ]
        let result = try fixture.execute("verify_file_identity_v1", arguments).object
        XCTAssertEqual(try result.number("byte_count"), Double(data.count))
        XCTAssertFalse(try result.string("device").isEmpty)
        XCTAssertFalse(try result.string("inode").isEmpty)

        XCTAssertThrowsError(try fixture.execute(
            "verify_file_identity_v1",
            arguments.merging(["sha256": .string(String(repeating: "0", count: 64))]) {
                _, new in new
            }
        ))
        let link = fixture.root.appendingPathComponent("linked.png")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: file)
        XCTAssertThrowsError(try fixture.execute(
            "verify_file_identity_v1",
            arguments.merging(["name": .string(link.lastPathComponent)]) { _, new in new }
        ))
    }

    func testRejectsSymbolicLinkEscapeAndChangedRootIdentity() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let outside = FileManager.default.temporaryDirectory
            .appendingPathComponent("outside-\(UUID().uuidString)")
        try Data("secret".utf8).write(to: outside)
        defer { try? FileManager.default.removeItem(at: outside) }
        try FileManager.default.createSymbolicLink(
            at: fixture.root.appendingPathComponent("escape"),
            withDestinationURL: outside
        )
        XCTAssertThrowsError(try fixture.execute(
            "read_file_raw",
            ["path": .string("escape")]
        )) { error in
            XCTAssertEqual(error.localizedDescription, "路径超出当前项目范围")
        }

        var request = try fixture.request("list_dir", [:])
        request = .init(
            protocolVersion: request.protocolVersion,
            requestID: request.requestID,
            helperInstanceID: request.helperInstanceID,
            deadlineUnixMilliseconds: request.deadlineUnixMilliseconds,
            operation: request.operation,
            arguments: request.arguments,
            projectRootPath: request.projectRootPath,
            projectRootIdentity: .init(
                device: request.projectRootIdentity.device,
                inode: request.projectRootIdentity.inode &+ 1
            ),
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepPath: nil,
            ripgrepIdentity: nil
        )
        XCTAssertThrowsError(try ChatOSCodeReadWorkerEngine.execute(request))
    }

    func testExpiredRequestDoesNotTouchProject() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let base = try fixture.request("list_dir", [:])
        let expired = CodeReadWorkerRequest(
            protocolVersion: base.protocolVersion,
            requestID: base.requestID,
            helperInstanceID: base.helperInstanceID,
            deadlineUnixMilliseconds: 1,
            operation: base.operation,
            arguments: base.arguments,
            projectRootPath: base.projectRootPath,
            projectRootIdentity: base.projectRootIdentity,
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepPath: nil,
            ripgrepIdentity: nil
        )
        XCTAssertThrowsError(try ChatOSCodeReadWorkerEngine.execute(expired))
    }

    func testProjectBrowserOperationsPreserveWorkspaceShapesAndSearchColumns() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let project = fixture.root.appendingPathComponent("Nested", isDirectory: true)
        let sources = project.appendingPathComponent("Sources", isDirectory: true)
        let ignored = project.appendingPathComponent("node_modules", isDirectory: true)
        try FileManager.default.createDirectory(at: sources, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: ignored, withIntermediateDirectories: true)
        try Data("let MixedToken = 1\n".utf8)
            .write(to: sources.appendingPathComponent("MixedToken.swift"))
        try Data("MixedToken\n".utf8)
            .write(to: ignored.appendingPathComponent("MixedToken.txt"))

        let listing = try fixture.execute(
            "project_list_v1",
            ["path": .string("Nested")]
        ).object
        XCTAssertEqual(try listing.string("path"), "Nested")
        XCTAssertEqual(try listing.string("parent"), ".")
        let sourceEntry = try XCTUnwrap(try listing.array("entries").first {
            try $0.object.string("name") == "Sources"
        })
        XCTAssertEqual(try sourceEntry.object.string("path"), "Nested/Sources")
        XCTAssertTrue(try sourceEntry.object.bool("is_dir"))

        let entries = try fixture.execute(
            "project_search_entries_v1",
            [
                "path": .string("Nested"),
                "query": .string("mixedtoken"),
                "max_results": .number(500),
            ]
        ).object
        XCTAssertEqual(
            try entries.array("matches").map { try $0.object.string("path") },
            ["Nested/Sources/MixedToken.swift"]
        )

        let content = try fixture.execute(
            "search_text",
            [
                "path": .string("Nested"),
                "pattern": .string("mixedtoken"),
                "case_insensitive": .bool(true),
                "max_results": .number(500),
            ]
        ).object
        let contentMatch = try XCTUnwrap(try content.array("results").first)
        XCTAssertEqual(try contentMatch.object.string("path"), "Nested/Sources/MixedToken.swift")
        XCTAssertEqual(try contentMatch.object.number("column"), 5)
    }

    func testProjectMutationsCreateOverwriteAndDeleteWithinAuthorizedRoot() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }

        let directory = try fixture.execute(
            "project_create_directory_v1",
            ["path": .string("Sources/Nested"), "create_only": .bool(true)]
        ).object
        XCTAssertTrue(try directory.bool("created"))
        XCTAssertThrowsError(try fixture.execute(
            "project_create_directory_v1",
            ["path": .string("Sources/Nested"), "create_only": .bool(true)]
        )) { error in
            XCTAssertEqual(error.localizedDescription, "目标已经存在")
        }

        let created = try fixture.execute(
            "project_write_file_v1",
            [
                "path": .string("Sources/Nested/Value.swift"),
                "content": .string("let value = 1\n"),
                "create_only": .bool(true),
            ]
        ).object
        XCTAssertTrue(try created.bool("created"))
        XCTAssertEqual(try created.number("size"), 14)

        let overwritten = try fixture.execute(
            "project_write_file_v1",
            [
                "path": .string("Sources/Nested/Value.swift"),
                "content": .string("let value = 2\n"),
                "create_only": .bool(false),
            ]
        ).object
        XCTAssertFalse(try overwritten.bool("created"))
        XCTAssertEqual(
            try String(contentsOf: fixture.root.appendingPathComponent("Sources/Nested/Value.swift")),
            "let value = 2\n"
        )

        XCTAssertThrowsError(try fixture.execute(
            "project_delete_entry_v1",
            ["path": .string("Sources"), "recursive": .bool(false)]
        )) { error in
            XCTAssertEqual(error.localizedDescription, "目录不为空，请使用递归删除")
        }
        let deleted = try fixture.execute(
            "project_delete_entry_v1",
            ["path": .string("Sources"), "recursive": .bool(true)]
        ).object
        XCTAssertTrue(try deleted.bool("deleted"))
        XCTAssertFalse(FileManager.default.fileExists(
            atPath: fixture.root.appendingPathComponent("Sources").path
        ))
    }

    func testProjectMutationsRejectRootTraversalAndSymbolicLinkParents() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let outside = FileManager.default.temporaryDirectory
            .appendingPathComponent("outside-mutation-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: outside) }
        try FileManager.default.createSymbolicLink(
            at: fixture.root.appendingPathComponent("escape"),
            withDestinationURL: outside
        )

        XCTAssertThrowsError(try fixture.execute(
            "project_write_file_v1",
            ["path": .string("../outside.txt"), "content": .string("unsafe")]
        )) { error in
            XCTAssertEqual(error.localizedDescription, "路径超出当前项目范围")
        }
        XCTAssertThrowsError(try fixture.execute(
            "project_create_directory_v1",
            ["path": .string(".")]
        )) { error in
            XCTAssertEqual(error.localizedDescription, "不能修改授权工作区根目录")
        }
        XCTAssertThrowsError(try fixture.execute(
            "project_write_file_v1",
            ["path": .string("escape/escaped.txt"), "content": .string("unsafe")]
        )) { error in
            XCTAssertEqual(error.localizedDescription, "操作不能穿过符号链接")
        }
        XCTAssertFalse(FileManager.default.fileExists(
            atPath: outside.appendingPathComponent("escaped.txt").path
        ))
    }

    func testAttachmentOperationsResolveVerifyAndMaterialize() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let workspace = try Fixture()
        defer { workspace.dispose() }
        let attachment = try fixture.makeAttachment(Data("0123456789\n".utf8))

        var resolveArguments = attachment.arguments
        resolveArguments["offset"] = .number(2)
        resolveArguments["limit"] = .number(5)
        let resolved = try fixture.execute("attachment_resolve_v1", resolveArguments).object
        XCTAssertEqual(try resolved.string("content"), "23456")
        XCTAssertEqual(try resolved.number("offset"), 2)
        XCTAssertEqual(try resolved.number("next_offset"), 7)

        let verified = try fixture.execute(
            "attachment_verify_file_v1",
            attachment.arguments
        ).object
        XCTAssertEqual(try verified.string("relative_path"), attachment.relativePath)
        XCTAssertEqual(try verified.number("byte_count"), 11)

        var materializeArguments = attachment.arguments
        materializeArguments.merge(try workspace.identityArguments()) { _, new in new }
        let materialized = try fixture.execute(
            "attachment_materialize_v1",
            materializeArguments
        ).object
        let relativePath = try materialized.string("relative_path")
        XCTAssertEqual(
            try Data(contentsOf: workspace.root.appendingPathComponent(relativePath)),
            attachment.data
        )
    }

    func testAttachmentOperationsRejectTamperingAndVaultSymlinks() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let attachment = try fixture.makeAttachment(Data("trusted".utf8))
        try Data("changed".utf8).write(to: attachment.fileURL)
        XCTAssertThrowsError(try fixture.execute(
            "attachment_verify_file_v1",
            attachment.arguments
        )) { error in
            XCTAssertEqual(error.localizedDescription, "The local attachment failed its integrity check.")
        }

        try FileManager.default.removeItem(at: attachment.fileURL)
        let outside = fixture.root.appendingPathComponent("outside")
        try attachment.data.write(to: outside)
        try FileManager.default.createSymbolicLink(
            at: attachment.fileURL,
            withDestinationURL: outside
        )
        XCTAssertThrowsError(try fixture.execute(
            "attachment_verify_file_v1",
            attachment.arguments
        )) { error in
            XCTAssertEqual(
                error.localizedDescription,
                "The local attachment is unavailable or exceeds its safety limit."
            )
        }

        try FileManager.default.removeItem(at: attachment.fileURL.deletingLastPathComponent())
        try FileManager.default.createSymbolicLink(
            at: attachment.fileURL.deletingLastPathComponent(),
            withDestinationURL: fixture.root
        )
        XCTAssertThrowsError(try fixture.execute(
            "attachment_resolve_v1",
            attachment.arguments.merging([
                "offset": .number(0), "limit": .number(1),
            ]) { _, new in new }
        ))
    }

    func testAttachmentMaterializeRejectsChangedAndSymlinkTargets() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let workspace = try Fixture()
        defer { workspace.dispose() }
        let attachment = try fixture.makeAttachment(Data("immutable\n".utf8))
        var arguments = attachment.arguments
        arguments.merge(try workspace.identityArguments()) { _, new in new }
        let relativePath = try fixture.execute(
            "attachment_materialize_v1",
            arguments
        ).object.string("relative_path")
        let target = workspace.root.appendingPathComponent(relativePath)

        try Data("user edit\n".utf8).write(to: target)
        XCTAssertThrowsError(try fixture.execute("attachment_materialize_v1", arguments)) {
            XCTAssertEqual(
                $0.localizedDescription,
                "The workspace attachment copy has changed or is unsafe; it was not overwritten."
            )
        }

        try FileManager.default.removeItem(at: target)
        let outside = workspace.root.appendingPathComponent("outside")
        try attachment.data.write(to: outside)
        try FileManager.default.createSymbolicLink(at: target, withDestinationURL: outside)
        XCTAssertThrowsError(try fixture.execute("attachment_materialize_v1", arguments)) {
            XCTAssertEqual(
                $0.localizedDescription,
                "The workspace attachment copy has changed or is unsafe; it was not overwritten."
            )
        }
        XCTAssertEqual(try Data(contentsOf: outside), attachment.data)
    }

    func testAttachmentMaterializeReclaimsOnlySafeStaleImportOrphans() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let workspace = try Fixture()
        defer { workspace.dispose() }
        let attachment = try fixture.makeAttachment(Data("orphan recovery\n".utf8))
        var arguments = attachment.arguments
        arguments.merge(try workspace.identityArguments()) { _, new in new }
        let relativePath = try fixture.execute(
            "attachment_materialize_v1",
            arguments
        ).object.string("relative_path")
        let installed = workspace.root.appendingPathComponent(relativePath)
        let directory = installed.deletingLastPathComponent()
        let stale = importURL(in: directory)
        let fresh = importURL(in: directory)
        let locked = importURL(in: directory)
        let maliciousHardLink = importURL(in: directory)
        let installedHardLink = importURL(in: directory)
        let symbolicLink = importURL(in: directory)
        let malformed = directory.appendingPathComponent(".import-not-a-canonical-uuid")
        let unrelated = directory.appendingPathComponent("unrelated")

        for url in [stale, fresh, locked, malformed, unrelated] {
            try Data("temporary".utf8).write(to: url)
        }
        try FileManager.default.linkItem(at: unrelated, to: maliciousHardLink)
        try FileManager.default.linkItem(at: installed, to: installedHardLink)
        try FileManager.default.createSymbolicLink(
            at: symbolicLink,
            withDestinationURL: unrelated
        )
        for url in [stale, locked, maliciousHardLink, installedHardLink] {
            try makeImportStale(url)
        }
        let lockedDescriptor = locked.path.withCString {
            Darwin.open($0, O_RDWR | O_NOFOLLOW | O_CLOEXEC)
        }
        XCTAssertGreaterThanOrEqual(lockedDescriptor, 0)
        defer { if lockedDescriptor >= 0 { _ = Darwin.close(lockedDescriptor) } }
        XCTAssertEqual(flock(lockedDescriptor, LOCK_EX | LOCK_NB), 0)

        _ = try fixture.execute("attachment_materialize_v1", arguments)

        XCTAssertFalse(FileManager.default.fileExists(atPath: stale.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: installedHardLink.path))
        XCTAssertEqual(try Data(contentsOf: installed), attachment.data)
        for preserved in [fresh, locked, maliciousHardLink, symbolicLink, malformed] {
            XCTAssertTrue(
                FileManager.default.fileExists(atPath: preserved.path),
                "Unexpectedly removed \(preserved.lastPathComponent)"
            )
        }
    }

    func testAttachmentOperationsRejectChangedRootIdentities() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let workspace = try Fixture()
        defer { workspace.dispose() }
        let attachment = try fixture.makeAttachment(Data("identity".utf8))

        XCTAssertThrowsError(try fixture.execute(
            "attachment_resolve_v1",
            attachment.arguments.merging([
                "offset": .number(Double(UInt64.max)), "limit": .number(1),
            ]) { _, new in new }
        ))

        let base = try fixture.request("attachment_verify_file_v1", attachment.arguments)
        let changedRoot = CodeReadWorkerRequest(
            protocolVersion: base.protocolVersion,
            requestID: base.requestID,
            helperInstanceID: base.helperInstanceID,
            deadlineUnixMilliseconds: base.deadlineUnixMilliseconds,
            operation: base.operation,
            arguments: base.arguments,
            projectRootPath: base.projectRootPath,
            projectRootIdentity: .init(
                device: base.projectRootIdentity.device,
                inode: base.projectRootIdentity.inode &+ 1
            ),
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepPath: nil,
            ripgrepIdentity: nil
        )
        XCTAssertThrowsError(try ChatOSCodeReadWorkerEngine.execute(changedRoot))

        var destinationArguments = attachment.arguments
        destinationArguments.merge(try workspace.identityArguments()) { _, new in new }
        destinationArguments["destination_root_inode"] = .string("0")
        XCTAssertThrowsError(try fixture.execute(
            "attachment_materialize_v1",
            destinationArguments
        )) { error in
            XCTAssertEqual(
                error.localizedDescription,
                "The authorized attachment could not be imported into the task workspace."
            )
        }
    }

    private func importURL(in directory: URL) -> URL {
        directory.appendingPathComponent(".import-" + UUID().uuidString.lowercased())
    }

    private func makeImportStale(_ url: URL) throws {
        try FileManager.default.setAttributes(
            [.modificationDate: Date(timeIntervalSinceNow: -(25 * 60 * 60))],
            ofItemAtPath: url.path
        )
    }

    func testAttachmentAuthorizationStreamsMultipleFilesAndRejectsShortPayload() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let first = Data("first attachment".utf8)
        let second = Data(repeating: 0x4b, count: 96 * 1_024)
        let firstToken = UUID().uuidString.lowercased()
        let secondToken = UUID().uuidString.lowercased()
        let arguments: [String: CodeReadWorkerJSONValue] = [
            "owner_user_id": .string("owner-stream"),
            "conversation_id": .string("conversation-stream"),
            "items": .array([
                .object([
                    "token": .string(firstToken),
                    "byte_size": .number(Double(first.count)),
                ]),
                .object([
                    "token": .string(secondToken),
                    "byte_size": .number(Double(second.count)),
                ]),
            ]),
        ]
        let result = try fixture.executeStreaming(
            "attachment_authorize_v1",
            arguments,
            payload: first + second
        )
        guard case let .object(payload) = result,
              case let .array(items)? = payload["items"] else {
            return XCTFail("Expected streamed attachment authorization result")
        }
        XCTAssertEqual(items.count, 2)
        let directory = fixture.root
            .appendingPathComponent(sha256(Data("owner-stream".utf8)))
            .appendingPathComponent(sha256(Data("conversation-stream".utf8)))
        XCTAssertEqual(try Data(contentsOf: directory.appendingPathComponent(firstToken)), first)
        XCTAssertEqual(try Data(contentsOf: directory.appendingPathComponent(secondToken)), second)

        let shortToken = UUID().uuidString.lowercased()
        XCTAssertThrowsError(try fixture.executeStreaming(
            "attachment_authorize_v1",
            [
                "owner_user_id": .string("owner-stream"),
                "conversation_id": .string("short-stream"),
                "items": .array([.object([
                    "token": .string(shortToken),
                    "byte_size": .number(16),
                ])]),
            ],
            payload: Data(repeating: 0x1, count: 8),
            advertisedByteCount: 16
        ))
        let shortDirectory = fixture.root
            .appendingPathComponent(sha256(Data("owner-stream".utf8)))
            .appendingPathComponent(sha256(Data("short-stream".utf8)))
        if FileManager.default.fileExists(atPath: shortDirectory.path) {
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(
                atPath: shortDirectory.path
            ).isEmpty)
        }
    }

    func testTemporaryCleanupRejectsCallerSelectedScope() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let arguments: [String: CodeReadWorkerJSONValue] = [
            "minimum_age_seconds": .number(24 * 60 * 60),
            "scopes": .array([
                .object([
                    "path": .string("LocalAgent/Attachments"),
                    "maximum_depth": .number(2),
                ]),
                .object([
                    "path": .string("LocalAgent/local-agent.artifact-staging"),
                    "maximum_depth": .number(0),
                ]),
                .object([
                    "path": .string("untrusted-project-directory"),
                    "maximum_depth": .number(2),
                ]),
            ]),
        ]

        XCTAssertThrowsError(try fixture.execute("cleanup_stale_temporary_files_v1", arguments))
    }
}

private struct Fixture {
    let root: URL

    init() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("CodeReadWorker-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    func execute(
        _ operation: String,
        _ arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        try ChatOSCodeReadWorkerEngine.execute(request(operation, arguments))
    }

    func executeStreaming(
        _ operation: String,
        _ arguments: [String: CodeReadWorkerJSONValue],
        payload: Data,
        advertisedByteCount: UInt64? = nil
    ) throws -> CodeReadWorkerJSONValue {
        let base = try request(operation, arguments)
        let streaming = CodeReadWorkerRequest(
            protocolVersion: base.protocolVersion,
            requestID: base.requestID,
            helperInstanceID: base.helperInstanceID,
            deadlineUnixMilliseconds: base.deadlineUnixMilliseconds,
            operation: base.operation,
            arguments: base.arguments,
            projectRootPath: base.projectRootPath,
            projectRootIdentity: base.projectRootIdentity,
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepPath: nil,
            ripgrepIdentity: nil,
            binaryPayloadByteCount: advertisedByteCount ?? UInt64(payload.count)
        )
        var offset = 0
        return try ChatOSCodeReadWorkerEngine.executeStreaming(streaming) { maximumCount in
            guard offset < payload.count else { return Data() }
            let end = min(payload.count, offset + maximumCount)
            defer { offset = end }
            return payload.subdata(in: offset..<end)
        }
    }

    func request(
        _ operation: String,
        _ arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerRequest {
        var status = stat()
        XCTAssertEqual(root.path.withCString { Darwin.lstat($0, &status) }, 0)
        return .init(
            protocolVersion: ChatOSCodeReadWorkerProtocol.maximumVersion,
            requestID: UUID().uuidString,
            helperInstanceID: UUID().uuidString,
            deadlineUnixMilliseconds: Int64((Date().timeIntervalSince1970 + 10) * 1_000),
            operation: operation,
            arguments: arguments,
            projectRootPath: root.path,
            projectRootIdentity: .init(
                device: UInt64(status.st_dev),
                inode: UInt64(status.st_ino)
            ),
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepPath: nil,
            ripgrepIdentity: nil
        )
    }

    func identityArguments() throws -> [String: CodeReadWorkerJSONValue] {
        var status = stat()
        guard root.path.withCString({ Darwin.lstat($0, &status) }) == 0 else {
            throw ShapeError.invalid
        }
        return [
            "destination_root_path": .string(root.path),
            "destination_root_device": .string(String(status.st_dev)),
            "destination_root_inode": .string(String(status.st_ino)),
        ]
    }

    func makeAttachment(_ data: Data) throws -> AttachmentFixture {
        let owner = "owner-user"
        let conversation = "conversation"
        let token = UUID().uuidString.lowercased()
        let ownerComponent = sha256(Data(owner.utf8))
        let conversationComponent = sha256(Data(conversation.utf8))
        let directory = root
            .appendingPathComponent(ownerComponent, isDirectory: true)
            .appendingPathComponent(conversationComponent, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let file = directory.appendingPathComponent(token)
        try data.write(to: file)
        return .init(
            data: data,
            fileURL: file,
            relativePath: [ownerComponent, conversationComponent, token].joined(separator: "/"),
            arguments: [
                "owner_user_id": .string(owner),
                "conversation_id": .string(conversation),
                "token": .string(token),
                "display_name": .string("notes.txt"),
                "media_type": .string("text/plain"),
                "byte_size": .number(Double(data.count)),
                "sha256": .string(sha256(data)),
            ]
        )
    }

    func dispose() { try? FileManager.default.removeItem(at: root) }
}

private struct AttachmentFixture {
    let data: Data
    let fileURL: URL
    let relativePath: String
    let arguments: [String: CodeReadWorkerJSONValue]
}

private func sha256(_ data: Data) -> String {
    SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
}

private enum ShapeError: Error { case invalid }

private extension CodeReadWorkerJSONValue {
    var object: [String: CodeReadWorkerJSONValue] {
        get throws {
            guard case let .object(value) = self else { throw ShapeError.invalid }
            return value
        }
    }
}

private extension Dictionary where Key == String, Value == CodeReadWorkerJSONValue {
    func string(_ key: String) throws -> String {
        guard case let .string(value)? = self[key] else { throw ShapeError.invalid }
        return value
    }

    func number(_ key: String) throws -> Double {
        guard case let .number(value)? = self[key] else { throw ShapeError.invalid }
        return value
    }

    func array(_ key: String) throws -> [CodeReadWorkerJSONValue] {
        guard case let .array(value)? = self[key] else { throw ShapeError.invalid }
        return value
    }

    func bool(_ key: String) throws -> Bool {
        guard case let .bool(value)? = self[key] else { throw ShapeError.invalid }
        return value
    }
}
