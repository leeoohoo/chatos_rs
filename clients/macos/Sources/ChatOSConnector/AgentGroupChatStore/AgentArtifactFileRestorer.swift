import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerProtocol
import ChatOSCore
import Foundation

enum AgentArtifactFileRestorer {
    static func restore(
        _ reference: AgentArtifactFileReference,
        to destinationURL: URL,
        expectedBytes: Int,
        expectedSHA256: String,
        codeReadWorker: any CodeReadWorkerExecuting = CodeReadWorkerProcessClient.shared
    ) async throws {
        guard reference.fileURL.isFileURL,
              destinationURL.isFileURL,
              expectedBytes > 0,
              expectedBytes <= AgentCommunicationPolicy.standard.maximumDocumentBytes,
              reference.size == expectedBytes,
              reference.sha256.lowercased() == expectedSHA256.lowercased(),
              isSHA256(expectedSHA256) else {
            throw AgentArtifactFileRestoreError.invalidSource
        }
        try Task.checkCancellation()
        let scope: AgentArtifactWorkerPathScope
        do {
            scope = try AgentArtifactWorkerPathScope(
                source: reference.fileURL,
                destination: destinationURL
            )
        } catch {
            throw AgentArtifactFileRestoreError.invalidSource
        }
        do {
            let result = try await codeReadWorker.execute(
                operation: "artifact_restore_v1",
                arguments: [
                    "source_path": .string(scope.sourcePath),
                    "destination_path": .string(scope.destinationPath),
                    "byte_size": .number(Double(expectedBytes)),
                    "sha256": .string(expectedSHA256.lowercased()),
                ],
                projectRoot: scope.root,
                requestCWD: nil,
                defaultToolRoot: nil,
                ripgrepURL: nil
            )
            guard case let .object(fields) = result,
                  case let .string(relativePath)? = fields["relative_path"],
                  relativePath == scope.destinationPath,
                  case let .number(byteCount)? = fields["byte_count"],
                  byteCount == Double(expectedBytes) else {
                throw AgentArtifactFileRestoreError.invalidSource
            }
        } catch is CancellationError {
            throw CancellationError()
        } catch let error as AgentArtifactFileRestoreError {
            throw error
        } catch {
            throw AgentArtifactFileRestoreError.invalidSource
        }
    }

    private static func isSHA256(_ value: String) -> Bool {
        value.utf8.count == 64 && value.utf8.allSatisfy {
            (48...57).contains($0) || (65...70).contains($0) || (97...102).contains($0)
        }
    }
}

enum AgentArtifactFileRestoreError: Error {
    case invalidSource
}
