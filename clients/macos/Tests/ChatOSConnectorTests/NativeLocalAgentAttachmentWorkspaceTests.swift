@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentAttachmentWorkspaceTests: XCTestCase {
    func testInlineDeliveryCannotDumpLargeBase64Result() async throws {
        let fixture = try AttachmentDispatchFixture(bytes: Data(repeating: 0xff, count: 120_000))
        defer { fixture.cleanup() }
        let result = try await fixture.executor().execute(
            ownerUserID: "user-1", invocation: fixture.invocation(delivery: "content")
        )
        guard case let .object(object) = result else { return XCTFail("Invalid result") }
        XCTAssertEqual(object["next_offset"], .number(2_048))
        XCTAssertEqual(object["encoding"], .string("base64"))
        XCTAssertEqual(object["content"], .string(fixture.bytes.prefix(2_048).base64EncodedString()))
        XCTAssertLessThan(try JSONEncoder().encode(result).count, 8_000)
    }

    func testDefaultDeliveryImportsEntireLargeAttachmentWithSmallResult() async throws {
        let data = Data(repeating: 0xff, count: 120_000)
        let fixture = try AttachmentDispatchFixture(bytes: data)
        defer { fixture.cleanup() }
        let root = try workspace(in: fixture)
        let executor = try fixture.executor(workspaceResolver: AttachmentWorkspaceRoot(root: root))
        // Omit delivery, as immutable old task tool schemas do. Offset/limit
        // must not turn the imported source file into a partial file.
        let invocation = fixture.invocation(offset: 5, limit: 3, delivery: nil)
        let result = try await executor.execute(ownerUserID: "user-1", invocation: invocation)
        let path = try relativePath(result)
        XCTAssertEqual(try Data(contentsOf: root.appendingPathComponent(path)), data)
        guard case let .object(object) = result else { return XCTFail("Invalid result") }
        XCTAssertEqual(object["sha256_verified"], .bool(true))
        XCTAssertEqual(object["byte_size"], .number(120_000))
        XCTAssertEqual(object["sha256"], .string(fixture.spec.sha256))
        XCTAssertNil(object["content"])
        XCTAssertLessThan(try JSONEncoder().encode(result).count, 1_024)
        let again = try await executor.execute(ownerUserID: "user-1", invocation: invocation)
        XCTAssertEqual(again, result)
        let source = try sourceURL(fixture)
        XCTAssertEqual(try Data(contentsOf: source), data)
    }

    func testConcurrentImportsAreIdempotent() async throws {
        let fixture = try AttachmentDispatchFixture()
        defer { fixture.cleanup() }
        let root = try workspace(in: fixture)
        let executor = try fixture.executor(workspaceResolver: AttachmentWorkspaceRoot(root: root))
        let invocation = fixture.invocation(delivery: "workspace")
        async let first = executor.execute(ownerUserID: "user-1", invocation: invocation)
        async let second = executor.execute(ownerUserID: "user-1", invocation: invocation)
        let results = try await [first, second]
        XCTAssertEqual(results[0], results[1])
        let path = try relativePath(results[0])
        XCTAssertEqual(try Data(contentsOf: root.appendingPathComponent(path)), fixture.bytes)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.appendingPathComponent(path).deletingLastPathComponent().path), ["source.docx"])
    }

    func testChangedWorkspaceCopyIsNotOverwritten() async throws {
        let fixture = try AttachmentDispatchFixture()
        defer { fixture.cleanup() }
        let root = try workspace(in: fixture)
        let executor = try fixture.executor(workspaceResolver: AttachmentWorkspaceRoot(root: root))
        let invocation = fixture.invocation(delivery: nil)
        let path = try relativePath(try await executor.execute(ownerUserID: "user-1", invocation: invocation))
        let file = root.appendingPathComponent(path)
        let edited = Data(repeating: 0, count: fixture.bytes.count)
        try edited.write(to: file)
        do {
            _ = try await executor.execute(ownerUserID: "user-1", invocation: invocation)
            XCTFail("Changed input copy must not be overwritten")
        } catch {
            XCTAssertEqual(error as? NativeLocalAgentAttachmentWorkspaceError, .conflictingFile)
        }
        XCTAssertEqual(try Data(contentsOf: file), edited)
        XCTAssertEqual(try Data(contentsOf: sourceURL(fixture)), fixture.bytes)
    }

    func testSymlinkImportDirectoryCannotEscapeWorkspace() async throws {
        let fixture = try AttachmentDispatchFixture()
        defer { fixture.cleanup() }
        let root = try workspace(in: fixture)
        let outside = fixture.root.appendingPathComponent("outside", isDirectory: true)
        try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: root.appendingPathComponent(".chatos-attachments"), withDestinationURL: outside)
        let executor = try fixture.executor(workspaceResolver: AttachmentWorkspaceRoot(root: root))
        do {
            _ = try await executor.execute(ownerUserID: "user-1", invocation: fixture.invocation(delivery: nil))
            XCTFail("Symlink directories must not be followed")
        } catch {
            XCTAssertEqual(error as? NativeLocalAgentAttachmentWorkspaceError, .unavailable)
        }
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: outside.path).isEmpty)
    }

    func testSymlinkSourceCopyIsRejectedWithoutModifyingTarget() async throws {
        let fixture = try AttachmentDispatchFixture()
        defer { fixture.cleanup() }
        let root = try workspace(in: fixture)
        let executor = try fixture.executor(workspaceResolver: AttachmentWorkspaceRoot(root: root))
        let invocation = fixture.invocation(delivery: nil)
        let path = try relativePath(try await executor.execute(ownerUserID: "user-1", invocation: invocation))
        let file = root.appendingPathComponent(path)
        let outside = fixture.root.appendingPathComponent("outside.docx")
        let targetBytes = Data("do not overwrite".utf8)
        try targetBytes.write(to: outside)
        try FileManager.default.removeItem(at: file)
        try FileManager.default.createSymbolicLink(at: file, withDestinationURL: outside)
        do {
            _ = try await executor.execute(ownerUserID: "user-1", invocation: invocation)
            XCTFail("Symlink source copies must be rejected")
        } catch {
            XCTAssertEqual(error as? NativeLocalAgentAttachmentWorkspaceError, .conflictingFile)
        }
        XCTAssertEqual(try Data(contentsOf: outside), targetBytes)
    }

    func testTamperedVaultNeverCreatesWorkspaceFile() async throws {
        let fixture = try AttachmentDispatchFixture()
        defer { fixture.cleanup() }
        let root = try workspace(in: fixture)
        try Data(repeating: 0, count: fixture.bytes.count).write(to: sourceURL(fixture))
        let executor = try fixture.executor(workspaceResolver: AttachmentWorkspaceRoot(root: root))
        do {
            _ = try await executor.execute(ownerUserID: "user-1", invocation: fixture.invocation(delivery: nil))
            XCTFail("Tampered vault content must not be imported")
        } catch {
            XCTAssertEqual(error as? NativeLocalAgentAttachmentVaultError, .integrityMismatch)
        }
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
    }

    func testUnauthorizedReferenceNeverCreatesWorkspaceFile() async throws {
        let fixture = try AttachmentDispatchFixture()
        defer { fixture.cleanup() }
        let root = try workspace(in: fixture)
        let executor = try fixture.executor(workspaceResolver: AttachmentWorkspaceRoot(root: root))
        do {
            _ = try await executor.execute(ownerUserID: "user-1", invocation: fixture.invocation(reference: "unknown", delivery: nil))
            XCTFail("Unregistered attachments must not be imported")
        } catch {
            XCTAssertEqual(error as? NativeLocalAgentPlatformToolError, .attachmentNotAuthorized)
        }
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
    }

    func testMaliciousDisplayNameCannotControlImportPath() async throws {
        let fixture = try AttachmentDispatchFixture(name: "../../outside.docx")
        defer { fixture.cleanup() }
        let root = try workspace(in: fixture)
        let result = try await fixture.executor(workspaceResolver: AttachmentWorkspaceRoot(root: root)).execute(
            ownerUserID: "user-1", invocation: fixture.invocation(delivery: nil)
        )
        let path = try relativePath(result)
        XCTAssertFalse(path.split(separator: "/").contains(".."))
        XCTAssertTrue(path.hasSuffix("/source.docx"))
        XCTAssertEqual(try Data(contentsOf: root.appendingPathComponent(path)), fixture.bytes)
    }

    func testActualDocumentPluginInspectsImportedPath() async throws {
        guard ProcessInfo.processInfo.environment["CHATOS_ATTACHMENT_PLUGIN_INTEGRATION"] == "1" else {
            throw XCTSkip("Enable for integration with the repository's document plugin and Node runtime")
        }
        let bytes = try runPluginFixture(["fixture"])
        let fixture = try AttachmentDispatchFixture(bytes: bytes)
        defer { fixture.cleanup() }
        let root = try workspace(in: fixture)
        let result = try await fixture.executor(workspaceResolver: AttachmentWorkspaceRoot(root: root)).execute(
            ownerUserID: "user-1", invocation: fixture.invocation(delivery: nil)
        )
        let path = try relativePath(result)
        let inspection = try runPluginFixture(["inspect", root.path, path])
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: inspection) as? [String: Any])
        XCTAssertEqual(object["ok"] as? Bool, true)
        let source = try XCTUnwrap(object["source"] as? [String: Any])
        XCTAssertEqual(source["sha256"] as? String, fixture.spec.sha256)
        XCTAssertEqual(source["size"] as? Int, bytes.count)
        XCTAssertEqual(source["relativePath"] as? String, path)
    }

    private func workspace(in fixture: AttachmentDispatchFixture) throws -> URL {
        let root = fixture.root.appendingPathComponent("workspace", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    private func relativePath(_ result: LocalAgentJSONValue) throws -> String {
        guard case let .object(object) = result,
              case let .string(path)? = object["workspace_relative_path"] else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return path
    }

    private func sourceURL(_ fixture: AttachmentDispatchFixture) throws -> URL {
        let token = String(fixture.spec.authorizedLocalRef.dropFirst(NativeLocalAgentAttachmentVault.referencePrefix.count))
        return fixture.root.appendingPathComponent(NativePluginManifestLoader.sha256("user-1"))
            .appendingPathComponent(NativePluginManifestLoader.sha256("conversation-1"))
            .appendingPathComponent(token)
    }

    private func runPluginFixture(_ arguments: [String]) throws -> Data {
        let repository = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["node", repository.appendingPathComponent("clients/macos/Tests/Integration/attachment-document.mjs").path] + arguments
        let output = Pipe()
        process.standardOutput = output
        process.standardError = FileHandle.standardError
        try process.run()
        let data = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        XCTAssertEqual(process.terminationStatus, 0)
        return data
    }
}

private struct AttachmentWorkspaceRoot: NativeLocalAgentAttachmentWorkspaceResolving {
    let root: URL
    func attachmentWorkspaceRoot(ownerUserID: String, runID: String) async throws -> URL { root }
}
