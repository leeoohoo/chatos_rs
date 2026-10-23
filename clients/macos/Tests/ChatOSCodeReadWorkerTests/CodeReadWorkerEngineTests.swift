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
}
