@testable import ChatOSConnector
import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerCore
import ChatOSCodeReadWorkerProtocol
import Darwin
import Foundation
import XCTest

final class NativeProjectRunAnalyzerTests: XCTestCase {
    func testDetectsNodeAndSwiftTargetsLocally() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-swift-run-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data(#"{"scripts":{"dev":"vite","test":"vitest"}}"#.utf8)
            .write(to: root.appendingPathComponent("package.json"))
        let swiftPackage = root.appendingPathComponent("Native", isDirectory: true)
        try FileManager.default.createDirectory(at: swiftPackage, withIntermediateDirectories: true)
        try Data("// swift-tools-version: 6.2\n".utf8)
            .write(to: swiftPackage.appendingPathComponent("Package.swift"))
        let executionCache = root.appendingPathComponent(".chatos/executions/run/frontend", isDirectory: true)
        try FileManager.default.createDirectory(at: executionCache, withIntermediateDirectories: true)
        try Data(#"{"scripts":{"start":"fake-cache-target"}}"#.utf8)
            .write(to: executionCache.appendingPathComponent("package.json"))

        let result = try await NativeProjectRunAnalyzer(
            codeReadWorker: ProjectRunInProcessCodeReadWorker()
        ).analyze(root: root)

        XCTAssertTrue(result.targets.contains(where: { $0.command == "npm run dev" }))
        XCTAssertTrue(result.targets.contains(where: { $0.command == "swift run" }))
        XCTAssertEqual(result.targets.first?.command, "npm run dev")
        XCTAssertTrue(result.configurationFiles.contains(where: { $0.path == "package.json" }))
        XCTAssertTrue(result.configurationFiles.contains(where: { $0.path == "Native/Package.swift" }))
        XCTAssertFalse(result.configurationFiles.contains(where: { $0.path.contains(".chatos") }))
    }

    func testLargePackageManifestIsNotLoadedForDetectionOrPreview() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-run-analyzer-large-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data(repeating: UInt8(ascii: "x"), count: 2 * 1_024 * 1_024 + 1)
            .write(to: root.appendingPathComponent("package.json"))

        let result = try await NativeProjectRunAnalyzer(
            codeReadWorker: ProjectRunInProcessCodeReadWorker()
        ).analyze(root: root)

        XCTAssertFalse(result.targets.contains(where: { $0.kind == "node" }))
        XCTAssertEqual(
            result.configurationFiles.first(where: { $0.path == "package.json" })?.preview,
            nil
        )
    }

    func testManifestDetectionDoesNotDependOnDirectoryEnumerationBudget() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-run-analyzer-bounded-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        for index in 0..<4 {
            try Data().write(to: root.appendingPathComponent("unrelated-\(index).txt"))
        }
        try Data(#"{"scripts":{"dev":"vite"}}"#.utf8)
            .write(to: root.appendingPathComponent("package.json"))

        let result = try await NativeProjectRunAnalyzer(
            maximumChildrenPerDirectory: 1,
            codeReadWorker: ProjectRunInProcessCodeReadWorker()
        )
            .analyze(root: root)

        XCTAssertTrue(result.targets.contains(where: { $0.command == "npm run dev" }))
    }
}

struct ProjectRunInProcessCodeReadWorker: CodeReadWorkerExecuting {
    func execute(
        operation: String,
        arguments: [String: CodeReadWorkerJSONValue],
        projectRoot: URL,
        requestCWD: String?,
        defaultToolRoot: String?,
        ripgrepURL: URL?
    ) async throws -> CodeReadWorkerJSONValue {
        var status = stat()
        guard projectRoot.path.withCString({ Darwin.lstat($0, &status) }) == 0 else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        return try ChatOSCodeReadWorkerEngine.execute(.init(
            protocolVersion: ChatOSCodeReadWorkerProtocol.maximumVersion,
            requestID: UUID().uuidString,
            helperInstanceID: UUID().uuidString,
            deadlineUnixMilliseconds: Int64((Date().timeIntervalSince1970 + 10) * 1_000),
            operation: operation,
            arguments: arguments,
            projectRootPath: projectRoot.path,
            projectRootIdentity: .init(
                device: UInt64(status.st_dev),
                inode: UInt64(status.st_ino)
            ),
            requestCWD: requestCWD,
            defaultToolRoot: defaultToolRoot,
            ripgrepPath: ripgrepURL?.path,
            ripgrepIdentity: nil
        ))
    }
}
