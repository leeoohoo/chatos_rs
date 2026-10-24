import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerProtocol
import ChatOSCore
import Foundation

enum NativeCodeReadWorkerBridge {
    static func execute(
        name: String,
        arguments: [String: NativeJSONValue],
        projectRoot: URL,
        requestCWD: String?,
        defaultToolRoot: String?,
        client: any CodeReadWorkerExecuting = CodeReadWorkerProcessClient.shared
    ) async throws -> NativeJSONValue {
        guard ChatOSCodeReadWorkerProtocol.operations.contains(name) else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        let result = try await client.execute(
            operation: name,
            arguments: arguments.mapValues(CodeReadWorkerJSONValue.init(native:)),
            projectRoot: projectRoot,
            requestCWD: requestCWD,
            defaultToolRoot: defaultToolRoot,
            ripgrepURL: NativeBundledToolLocator.executable(named: "rg")
        )
        let nativeResult = NativeJSONValue(worker: result)
        guard name == "open_file_in_pet" else { return nativeResult }
        guard case let .object(payload) = nativeResult,
              case let .string(relativePath)? = payload["relative_path"],
              case let .string(mode)? = payload["mode"],
              relativePath != ".", !relativePath.hasPrefix("/"),
              !relativePath.split(separator: "/").contains("..") else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        let line: Int?
        switch payload["line"] {
        case let .number(value):
            guard value.isFinite, value >= 1, value.rounded() == value,
                  value <= 9_007_199_254_740_991 else {
                throw CodeReadWorkerProcessClientError.invalidResponse
            }
            line = Int(value)
        case .null:
            line = nil
        default:
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        guard mode == "preview" || mode == "edit" else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        let url = projectRoot.appendingPathComponent(relativePath).standardizedFileURL
        let root = projectRoot.standardizedFileURL.resolvingSymlinksInPath()
        let resolved = url.resolvingSymlinksInPath()
        let prefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
        guard resolved.path.hasPrefix(prefix) else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        let request = PetFilePresentationRequest(
            path: resolved.path,
            targetLine: line,
            prefersEditing: mode == "edit"
        )
        await MainActor.run {
            NotificationCenter.default.post(
                name: .chatOSPetOpenFileRequested,
                object: request
            )
        }
        return .object([
            "presented": .bool(true),
            "path": .string(resolved.path),
            "line": line.map { .number(Double($0)) } ?? .null,
            "mode": .string(mode),
        ])
    }
}

private extension CodeReadWorkerJSONValue {
    init(native value: NativeJSONValue) {
        switch value {
        case .null: self = .null
        case let .bool(value): self = .bool(value)
        case let .number(value): self = .number(value)
        case let .string(value): self = .string(value)
        case let .array(value): self = .array(value.map(Self.init(native:)))
        case let .object(value): self = .object(value.mapValues(Self.init(native:)))
        }
    }
}

private extension NativeJSONValue {
    init(worker value: CodeReadWorkerJSONValue) {
        switch value {
        case .null: self = .null
        case let .bool(value): self = .bool(value)
        case let .number(value): self = .number(value)
        case let .string(value): self = .string(value)
        case let .array(value): self = .array(value.map(Self.init(worker:)))
        case let .object(value): self = .object(value.mapValues(Self.init(worker:)))
        }
    }
}
