@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeWorkspaceFilesystemTests: XCTestCase {
    func testDirectFilesystemOperationsStayInsideWorkspace() throws {
        let root = try temporaryDirectory(named: "filesystem")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root.appendingPathComponent("Sources"), withIntermediateDirectories: true)
        try Data("hello Swift\n".utf8).write(to: root.appendingPathComponent("Sources/main.swift"))
        let filesystem = NativeWorkspaceFilesystem(workspace: workspace(root))

        let listing = try filesystem.list(path: ".", includeFiles: true)
        guard case let .object(listObject) = listing,
              case let .array(entries)? = listObject["entries"] else {
            return XCTFail("expected directory listing")
        }
        XCTAssertEqual(entries.count, 1)

        let read = try filesystem.read(path: "Sources/main.swift")
        guard case let .object(readObject) = read,
              case let .string(content)? = readObject["content"] else {
            return XCTFail("expected file content")
        }
        XCTAssertEqual(content, "hello Swift\n")

        _ = try filesystem.write(path: "Sources/New.swift", content: "new", createOnly: true)
        _ = try filesystem.move(
            sourcePath: "Sources/New.swift",
            targetPath: "New.swift",
            replaceExisting: false
        )
        XCTAssertTrue(FileManager.default.fileExists(atPath: root.appendingPathComponent("New.swift").path))
        _ = try filesystem.delete(path: "New.swift", recursive: false)
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("New.swift").path))

        XCTAssertThrowsError(try filesystem.read(path: "../outside.txt"))
        XCTAssertThrowsError(try filesystem.write(path: "../outside.txt", content: "no", createOnly: false))
        XCTAssertThrowsError(try filesystem.delete(path: ".", recursive: true))
    }

    func testSearchSkipsSymlinkThatEscapesWorkspace() throws {
        let root = try temporaryDirectory(named: "search")
        let outside = try temporaryDirectory(named: "outside")
        defer {
            try? FileManager.default.removeItem(at: root)
            try? FileManager.default.removeItem(at: outside)
        }
        try Data("secret needle".utf8).write(to: outside.appendingPathComponent("secret.txt"))
        try FileManager.default.createSymbolicLink(
            at: root.appendingPathComponent("escaped"),
            withDestinationURL: outside
        )
        try Data("local needle".utf8).write(to: root.appendingPathComponent("local.txt"))
        let filesystem = NativeWorkspaceFilesystem(workspace: workspace(root))

        let result = try filesystem.searchContent(path: ".", query: "needle", limit: 20)
        guard case let .object(object) = result,
              case let .array(matches)? = object["matches"] else {
            return XCTFail("expected content matches")
        }
        XCTAssertEqual(matches.count, 1)
        guard case let .object(match) = matches[0],
              case let .string(path)? = match["path"] else {
            return XCTFail("expected match path")
        }
        XCTAssertEqual(path, "local.txt")
    }

    func testSearchSkipsGeneratedDependencyDirectories() throws {
        let root = try temporaryDirectory(named: "ignored-search-folders")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(
            at: root.appendingPathComponent("node_modules/package"),
            withIntermediateDirectories: true
        )
        try FileManager.default.createDirectory(
            at: root.appendingPathComponent("Sources"),
            withIntermediateDirectories: true
        )
        try Data("needle dependency".utf8)
            .write(to: root.appendingPathComponent("node_modules/package/index.js"))
        try Data("needle source".utf8)
            .write(to: root.appendingPathComponent("Sources/main.swift"))
        let filesystem = NativeWorkspaceFilesystem(workspace: workspace(root))

        let result = try filesystem.searchContent(path: ".", query: "needle", limit: 20)
        guard case let .object(object) = result,
              case let .array(matches)? = object["matches"] else {
            return XCTFail("expected content matches")
        }
        XCTAssertEqual(matches.count, 1)
        guard case let .object(match) = matches[0],
              case let .string(path)? = match["path"] else {
            return XCTFail("expected match path")
        }
        XCTAssertEqual(path, "Sources/main.swift")
    }

    private func workspace(_ root: URL) -> LocalConnectorWorkspace {
        .init(id: "workspace", alias: "test", absoluteRoot: root.path, fingerprint: "fingerprint")
    }

    private func temporaryDirectory(named name: String) throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-swift-\(name)-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }
}
