import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    func handleWorkspaceRelayMessage(
        _ data: Data,
        socket: URLSessionWebSocketTask
    ) async {
        let decoded = try? JSONDecoder().decode(NativeRelayRequest.self, from: data)
        let requestID = decoded?.requestID ?? ""
        let responseType = Self.workspaceResponseType(for: decoded?.type)
        do {
            guard let request = decoded else { throw NativeWorkspaceRelayError.unsupportedRequest }
            let response = try await processWorkspaceRelay(request)
            try await sendRelayResponse(response, socket: socket)
        } catch {
            let status = (error as? NativeWorkspaceRelayError)?.status ?? 400
            let response = NativeRelayResponse(
                type: responseType,
                requestID: requestID,
                status: status,
                body: .object(["error": .string(error.localizedDescription)])
            )
            try? await sendRelayResponse(response, socket: socket)
        }
    }

    private func processWorkspaceRelay(_ request: NativeRelayRequest) async throws -> NativeRelayResponse {
        guard let ownerUserID = state.user?.id,
              let deviceID = state.deviceID,
              let workspace = state.workspaces.first(where: { $0.id == request.workspaceID }) else {
            throw NativeWorkspaceRelayError.invalidContext
        }
        let token = try requireAccessToken()
        let runtime = try await gateway.managedRuntimeConfig(token: token)
        try NativeRelayVerifier().verify(
            request,
            trust: runtime.remoteControlTrust,
            ownerUserID: ownerUserID,
            deviceID: deviceID,
            seenNonces: &seenRelayNonces
        )

        let body: NativeJSONValue
        switch request.type {
        case "workspace_directory_list_request":
            let requestBody = try request.body.decode(NativeWorkspaceDirectoryBody.self)
            body = try await Task.detached {
                try NativeWorkspaceFilesystem(workspace: workspace)
                    .list(path: requestBody.path ?? ".", includeFiles: false)
            }.value
        case "workspace_directory_create_request":
            let requestBody = try request.body.decode(NativeWorkspaceDirectoryBody.self)
            guard let path = requestBody.path?.trimmingCharacters(in: .whitespacesAndNewlines),
                  !path.isEmpty else { throw NativeWorkspaceRelayError.missingField("path") }
            body = try await Task.detached {
                try NativeWorkspaceFilesystem(workspace: workspace).createDirectory(path: path)
            }.value
        case "workspace_filesystem_request":
            let operation = try request.body.decode(NativeWorkspaceFilesystemRequest.self)
            body = try await Task.detached {
                try Self.performFilesystemOperation(operation, workspace: workspace)
            }.value
        default:
            throw NativeWorkspaceRelayError.unsupportedRequest
        }
        return .init(
            type: Self.workspaceResponseType(for: request.type),
            requestID: request.requestID,
            status: 200,
            body: body
        )
    }

    nonisolated private static func performFilesystemOperation(
        _ request: NativeWorkspaceFilesystemRequest,
        workspace: LocalConnectorWorkspace
    ) throws -> NativeJSONValue {
        let filesystem = NativeWorkspaceFilesystem(workspace: workspace)
        switch request.operation {
        case "list":
            return try filesystem.list(path: request.path ?? ".", includeFiles: true)
        case "read":
            return try filesystem.read(path: try required(request.path, field: "path"))
        case "search_entries":
            return try filesystem.searchEntries(
                path: request.path ?? ".",
                query: try required(request.query, field: "query"),
                limit: request.limit ?? 200
            )
        case "search_content":
            return try filesystem.searchContent(
                path: request.path ?? ".",
                query: try required(request.query, field: "query"),
                limit: request.limit ?? 200
            )
        case "create_directory":
            return try filesystem.createDirectory(path: try required(request.path, field: "path"))
        case "create_file":
            return try filesystem.write(
                path: try required(request.path, field: "path"),
                content: request.content ?? "",
                createOnly: true
            )
        case "write_file":
            return try filesystem.write(
                path: try required(request.path, field: "path"),
                content: try required(request.content, field: "content"),
                createOnly: false
            )
        case "delete":
            return try filesystem.delete(
                path: try required(request.path, field: "path"),
                recursive: request.recursive ?? false
            )
        case "move":
            return try filesystem.move(
                sourcePath: try required(request.sourcePath, field: "source_path"),
                targetPath: try required(request.targetPath, field: "target_path"),
                replaceExisting: request.replaceExisting ?? false
            )
        default:
            throw NativeWorkspaceRelayError.unsupportedOperation(request.operation)
        }
    }

    nonisolated private static func required(_ value: String?, field: String) throws -> String {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines), !value.isEmpty else {
            throw NativeWorkspaceRelayError.missingField(field)
        }
        return value
    }

    nonisolated private static func workspaceResponseType(for requestType: String?) -> String {
        switch requestType {
        case "workspace_directory_list_request": "workspace_directory_list_response"
        case "workspace_directory_create_request": "workspace_directory_create_response"
        default: "workspace_filesystem_response"
        }
    }
}
