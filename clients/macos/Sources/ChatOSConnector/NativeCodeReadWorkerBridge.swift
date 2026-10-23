import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerProtocol
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
        return NativeJSONValue(worker: result)
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
