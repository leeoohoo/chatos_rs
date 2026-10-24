import ChatOSCodeReadWorkerCore
import ChatOSCodeReadWorkerProtocol
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
            ["path": .string("Sources/Nested")]
        ).object
        XCTAssertTrue(try directory.bool("created"))

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

    func dispose() { try? FileManager.default.removeItem(at: root) }
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
