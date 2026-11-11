import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerProtocol
import ChatOSCore
import Darwin
import Foundation

struct StagedAgentArtifact: Sendable, Equatable {
    let token: String
    let fileURL: URL
}

enum AgentArtifactUploadStager {
    static func stage(
        from sourceURL: URL,
        in stagingDirectoryURL: URL,
        expectedBytes: Int,
        expectedSHA256: String,
        codeReadWorker: any CodeReadWorkerExecuting = CodeReadWorkerProcessClient.shared
    ) async throws -> StagedAgentArtifact {
        guard sourceURL.isFileURL,
              stagingDirectoryURL.isFileURL,
              expectedBytes > 0,
              expectedBytes <= AgentCommunicationPolicy.standard.maximumDocumentBytes,
              isSHA256(expectedSHA256) else {
            throw AgentArtifactUploadStagingError.invalidSource
        }
        try Task.checkCancellation()
        let token = UUID().uuidString.lowercased()
        let finalURL = stagingDirectoryURL.appendingPathComponent("\(token).staged")
        let scope: AgentArtifactWorkerPathScope
        do {
            scope = try AgentArtifactWorkerPathScope(source: sourceURL, destination: finalURL)
        } catch {
            throw AgentArtifactUploadStagingError.stagingUnavailable
        }
        do {
            let result = try await codeReadWorker.execute(
                operation: "artifact_stage_v2",
                arguments: scope.destinationArguments.merging([
                    "source_path": .string(scope.sourcePath),
                    "destination_path": .string(scope.destinationPath),
                    "token": .string(token),
                    "byte_size": .number(Double(expectedBytes)),
                    "sha256": .string(expectedSHA256.lowercased()),
                ]) { _, value in value },
                projectRoot: scope.root,
                requestCWD: nil,
                defaultToolRoot: nil,
                ripgrepURL: nil
            )
            guard case let .object(fields) = result,
                  case let .string(returnedToken)? = fields["token"],
                  returnedToken == token,
                  case let .string(relativePath)? = fields["relative_path"],
                  relativePath == scope.destinationPath else {
                throw AgentArtifactUploadStagingError.stagingUnavailable
            }
            return .init(token: token, fileURL: scope.destinationURL)
        } catch is CancellationError {
            throw CancellationError()
        } catch let error as CodeReadWorkerProcessClientError {
            if case let .requestRejected(code, _) = error, code == "artifact_invalid" {
                throw AgentArtifactUploadStagingError.invalidSource
            }
            throw AgentArtifactUploadStagingError.stagingUnavailable
        } catch let error as AgentArtifactUploadStagingError {
            throw error
        } catch {
            throw AgentArtifactUploadStagingError.stagingUnavailable
        }
    }

    static func discard(_ staged: StagedAgentArtifact) {
        guard staged.token.utf8.allSatisfy(isTokenByte),
              staged.fileURL.lastPathComponent == "\(staged.token).staged" else { return }
        _ = staged.fileURL.withUnsafeFileSystemRepresentation { path in
            path.map(Darwin.unlink) ?? -1
        }
    }

    private static func isSHA256(_ value: String) -> Bool {
        value.utf8.count == 64 && value.utf8.allSatisfy {
            (48...57).contains($0) || (65...70).contains($0) || (97...102).contains($0)
        }
    }

    private static func isTokenByte(_ byte: UInt8) -> Bool {
        (48...57).contains(byte) || (97...102).contains(byte) || byte == 45
    }
}

struct AgentArtifactWorkerPathScope {
    let root: URL
    let sourcePath: String
    let destinationPath: String
    let destinationRoot: URL
    let destinationIdentity: CodeReadWorkerFileIdentity

    var destinationURL: URL {
        destinationRoot.appendingPathComponent(destinationPath)
    }

    var destinationArguments: [String: CodeReadWorkerJSONValue] {
        [
            "destination_root_path": .string(destinationRoot.path),
            "destination_root_device": .string(String(destinationIdentity.device)),
            "destination_root_inode": .string(String(destinationIdentity.inode)),
        ]
    }

    init(source: URL, destination: URL) throws {
        let source = source.standardizedFileURL
        let destination = destination.standardizedFileURL
        guard source.isFileURL, destination.isFileURL else {
            throw AgentArtifactWorkerPathError.invalidScope
        }
        // Resolve configured directory aliases, never the file itself. Source and destination
        // can live on different volumes; granting their common ancestor would grant all of /.
        root = source.deletingLastPathComponent().resolvingSymlinksInPath()
        sourcePath = source.lastPathComponent
        var ancestor = destination.deletingLastPathComponent()
        var pending = [destination.lastPathComponent]
        var status = stat()
        while ancestor.path.withCString({ Darwin.lstat($0, &status) }) != 0 {
            guard errno == ENOENT, ancestor.path != "/" else {
                throw AgentArtifactWorkerPathError.invalidScope
            }
            pending.insert(ancestor.lastPathComponent, at: 0)
            ancestor.deleteLastPathComponent()
        }
        destinationRoot = ancestor.resolvingSymlinksInPath().standardizedFileURL
        guard destinationRoot.path.withCString({ Darwin.lstat($0, &status) }) == 0,
              (status.st_mode & S_IFMT) == S_IFDIR else {
            throw AgentArtifactWorkerPathError.invalidScope
        }
        destinationIdentity = .init(device: UInt64(status.st_dev), inode: UInt64(status.st_ino))
        destinationPath = pending.joined(separator: "/")
    }
}

private enum AgentArtifactWorkerPathError: Error {
    case invalidScope
}

enum AgentArtifactUploadStagingError: Error {
    case invalidSource
    case stagingUnavailable
}
