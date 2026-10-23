import Darwin
import ChatOSNetworking
import Foundation

enum AgentModelRequestBodyTransport {
    static func upload(
        request: URLRequest,
        bodyFile: AgentModelRequestBodyFile
    ) async throws -> (Data, Int) {
        try validate(bodyFile)
        let transferPermit = try await ClientLargeTransferGovernor.shared.acquire(
            context: .init(
                taskID: "agent-model-upload:\(UUID().uuidString.lowercased())",
                direction: .upload
            )
        )
        defer { transferPermit.release() }
        let (data, response) = try await BoundedURLSessionDataLoader.upload(
            request: request,
            fileURL: bodyFile.url,
            maximumBytes: 2 * 1_024 * 1_024,
            transferPermit: transferPermit
        )
        guard let response = response as? HTTPURLResponse else {
            throw AgentRuntimeError.invalidResponse
        }
        return (data, response.statusCode)
    }

    static func stream(
        request: URLRequest,
        bodyFile: AgentModelRequestBodyFile
    ) async throws -> AgentHTTPStreamResponse {
        try validate(bodyFile)
        let transferPermit = try await ClientLargeTransferGovernor.shared.acquire(
            context: .init(
                taskID: "agent-model-stream-upload:\(UUID().uuidString.lowercased())",
                direction: .upload
            )
        )
        do {
            return try await AgentURLSessionStream.open(
                request,
                bodyFileURL: bodyFile.url,
                transferPermit: transferPermit
            )
        } catch {
            transferPermit.release()
            throw error
        }
    }

    static func materialize(
        request: URLRequest,
        bodyFile: AgentModelRequestBodyFile
    ) throws -> URLRequest {
        try validate(bodyFile)
        var materialized = request
        materialized.httpBody = try Data(contentsOf: bodyFile.url, options: .mappedIfSafe)
        guard materialized.httpBody?.count == bodyFile.byteCount else {
            throw AgentRuntimeError.invalidAttachment
        }
        return materialized
    }

    private static func validate(_ bodyFile: AgentModelRequestBodyFile) throws {
        guard bodyFile.url.isFileURL,
              bodyFile.byteCount > 0,
              bodyFile.byteCount <= AgentModelRequestBodyFileWriter.maximumRequestBytes else {
            throw AgentRuntimeError.invalidAttachment
        }
        var status = stat()
        let result = bodyFile.url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.lstat(path, &status)
        }
        guard result == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_uid == geteuid(),
              status.st_nlink == 1,
              status.st_size == bodyFile.byteCount,
              status.st_mode & 0o077 == 0 else {
            throw AgentRuntimeError.invalidAttachment
        }
    }
}
