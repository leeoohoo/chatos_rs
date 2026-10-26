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
                operation: "artifact_stage_v1",
                arguments: [
                    "source_path": .string(scope.sourcePath),
                    "destination_path": .string(scope.destinationPath),
                    "token": .string(token),
                    "byte_size": .number(Double(expectedBytes)),
                    "sha256": .string(expectedSHA256.lowercased()),
                ],
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
            return .init(token: token, fileURL: finalURL)
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

    init(source: URL, destination: URL) throws {
        let source = source.standardizedFileURL
        let destination = destination.standardizedFileURL
        guard source.isFileURL, destination.isFileURL else {
            throw AgentArtifactWorkerPathError.invalidScope
        }
        let sourceComponents = source.pathComponents
        let destinationComponents = destination.pathComponents
        var sharedCount = 0
        while sharedCount < min(sourceComponents.count, destinationComponents.count),
              sourceComponents[sharedCount] == destinationComponents[sharedCount] {
            sharedCount += 1
        }
        guard sharedCount > 1,
              sharedCount < sourceComponents.count,
              sharedCount < destinationComponents.count else {
            throw AgentArtifactWorkerPathError.invalidScope
        }
        let rootPath = NSString.path(withComponents: Array(sourceComponents.prefix(sharedCount)))
        root = URL(fileURLWithPath: rootPath, isDirectory: true)
        sourcePath = sourceComponents.dropFirst(sharedCount).joined(separator: "/")
        destinationPath = destinationComponents.dropFirst(sharedCount).joined(separator: "/")
        guard !sourcePath.isEmpty, !destinationPath.isEmpty else {
            throw AgentArtifactWorkerPathError.invalidScope
        }
    }
}

private enum AgentArtifactWorkerPathError: Error {
    case invalidScope
}

enum AgentArtifactUploadStagingError: Error {
    case invalidSource
    case stagingUnavailable
}
