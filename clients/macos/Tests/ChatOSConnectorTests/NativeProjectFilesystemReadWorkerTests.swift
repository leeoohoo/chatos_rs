@testable import ChatOSConnector
import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerCore
import ChatOSCodeReadWorkerProtocol
import ChatOSCore
import Darwin
import Foundation
import XCTest

final class NativeProjectFilesystemReadWorkerTests: XCTestCase {
    func testProjectBrowserReadsUseWorkerAndRestoreLogicalPaths() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-project-browser-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let project = root.appendingPathComponent("nested-project", isDirectory: true)
        let sources = project.appendingPathComponent("Sources", isDirectory: true)
        try FileManager.default.createDirectory(at: sources, withIntermediateDirectories: true)
        try Data("let MixedToken = 1\n".utf8)
            .write(to: sources.appendingPathComponent("MixedToken.swift"))

        let connector = try makeConnector(root: root)
        let service = NativeProjectFilesystemService(
            connector: connector,
            codeReadWorker: FilesystemCoreCodeReadWorker()
        )
        let projectRoot = "local://connector/device/workspace/nested-project"

        let listing = try await service.listEntries(path: projectRoot, forceRefresh: false)
        XCTAssertEqual(listing.path, projectRoot)
        XCTAssertEqual(listing.parentPath, "local://connector/device/workspace")
        XCTAssertEqual(listing.entries.map(\.path), [projectRoot + "/Sources"])

        let entries = try await service.searchEntries(
            path: projectRoot,
            query: "mixedtoken",
            limit: 20
        )
        XCTAssertEqual(entries.map(\.path), [projectRoot + "/Sources/MixedToken.swift"])

        let content = try await service.searchContent(
            path: projectRoot,
            query: "mixedtoken",
            limit: 20
        )
        XCTAssertEqual(content.map(\.path), [projectRoot + "/Sources/MixedToken.swift"])
        XCTAssertEqual(content.first?.line, 1)
        XCTAssertEqual(content.first?.column, 5)
    }

    private func makeConnector(root: URL) throws -> NativeLocalConnectorService {
        let stateURL = root.appendingPathComponent("connector-state.json")
        var state = NativeConnectorPersistentState.empty
        state.deviceID = "device"
        state.workspaces = [
            .init(id: "workspace", alias: "test", absoluteRoot: root.path, fingerprint: "test"),
        ]
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        return NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!,
                stateURL: stateURL
            ),
            ticketProvider: FilesystemTicketProvider()
        )
    }
}

private struct FilesystemCoreCodeReadWorker: CodeReadWorkerExecuting {
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
        let ripgrepIdentity: CodeReadWorkerFileIdentity?
        if let ripgrepURL {
            var ripgrepStatus = stat()
            guard ripgrepURL.path.withCString({ Darwin.lstat($0, &ripgrepStatus) }) == 0 else {
                throw CodeReadWorkerProcessClientError.invalidResponse
            }
            ripgrepIdentity = .init(
                device: UInt64(ripgrepStatus.st_dev),
                inode: UInt64(ripgrepStatus.st_ino)
            )
        } else {
            ripgrepIdentity = nil
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
            ripgrepIdentity: ripgrepIdentity
        ))
    }
}

private struct FilesystemTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String { "unused" }
}
