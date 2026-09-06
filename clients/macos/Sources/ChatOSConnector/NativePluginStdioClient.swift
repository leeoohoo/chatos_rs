import Foundation
import OSLog

actor NativePluginStdioClient {
    private enum TimeoutBehavior: Sendable, Equatable {
        case terminateProcess
        case cancelRequest
    }

    private struct PendingRequest {
        var continuation: CheckedContinuation<NativeJSONValue, any Error>
        var timeoutBehavior: TimeoutBehavior
    }

    private let process: Process
    private let input: FileHandle
    private let output: FileHandle
    private let errorOutput: FileHandle
    private var outputReaderTask: Task<Void, Never>?
    private var errorReaderTask: Task<Void, Never>?
    private var nextRequestID = 1
    private var pending: [Int: PendingRequest] = [:]
    private var readBuffer = Data()
    private var errorBuffer = Data()
    private var stopped = false
    private let maximumMessageBytes = 8 * 1_024 * 1_024
    private let maximumErrorBytes = 16 * 1_024
    private static let logger = Logger(
        subsystem: "com.chatos.swift-client",
        category: "NativePluginStdioClient"
    )

    init(launch: NativePreparedPluginLaunch) {
        let inputPipe = Pipe()
        let outputPipe = Pipe()
        let errorPipe = Pipe()
        let process = Process()
        process.executableURL = launch.executableURL
        process.arguments = launch.arguments
        process.currentDirectoryURL = launch.installationURL
        process.standardInput = inputPipe
        process.standardOutput = outputPipe
        process.standardError = errorPipe
        process.environment = NativePluginProcessEnvironment.make(overrides: launch.environment)
        self.process = process
        self.input = inputPipe.fileHandleForWriting
        self.output = outputPipe.fileHandleForReading
        self.errorOutput = errorPipe.fileHandleForReading
    }

    func start() throws {
        guard !process.isRunning else { return }
        let outputStream = AsyncStream<Data> { continuation in
            output.readabilityHandler = { handle in
                let data = handle.availableData
                if data.isEmpty {
                    continuation.finish()
                } else {
                    continuation.yield(data)
                }
            }
        }
        let errorStream = AsyncStream<Data> { continuation in
            errorOutput.readabilityHandler = { handle in
                let data = handle.availableData
                if data.isEmpty {
                    continuation.finish()
                } else {
                    continuation.yield(data)
                }
            }
        }
        outputReaderTask = Task { [weak self] in
            for await data in outputStream {
                guard let self else { return }
                await self.consume(data)
            }
        }
        errorReaderTask = Task { [weak self] in
            for await data in errorStream {
                guard let self else { return }
                await self.consumeError(data)
            }
        }
        process.terminationHandler = { [weak self] process in
            Task { await self?.processEnded(exitCode: process.terminationStatus) }
        }
        try process.run()
    }

    func initialize() async throws -> (instructions: String?, tools: [NativeJSONValue]) {
        let result = try await request(
            method: "initialize",
            params: .object([
                "protocolVersion": .string("2024-11-05"),
                "capabilities": .object([:]),
                "clientInfo": .object([
                    "name": .string("ChatOS Swift"),
                    "version": .string("1"),
                ]),
            ]),
            timeout: .seconds(30)
        )
        let instructions = result.jsonObject?["instructions"]?.jsonString?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        try notify(method: "notifications/initialized", params: .object([:]))
        let list = try await request(method: "tools/list", params: .object([:]), timeout: .seconds(30))
        guard let tools = list.jsonObject?["tools"]?.jsonArray, !tools.isEmpty else {
            throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP 没有发布任何工具")
        }
        return (instructions?.isEmpty == false ? instructions : nil, tools)
    }

    func callTool(
        name: String,
        arguments: NativeJSONValue,
        timeout: Duration
    ) async throws -> NativeJSONValue {
        try await request(
            method: "tools/call",
            params: .object(["name": .string(name), "arguments": arguments]),
            timeout: timeout,
            timeoutBehavior: .terminateProcess
        )
    }

    /// Executes a cancellable MCP call without invalidating the whole plugin
    /// process when the call misses its deadline. The MCP cancellation
    /// notification lets compliant servers release any per-session resources,
    /// while a late response from a server that ignores cancellation is safely
    /// discarded. Use this only for MCP servers whose cancellation behavior is
    /// controlled and verified by ChatOS.
    func callToolBestEffort(
        name: String,
        arguments: NativeJSONValue,
        timeout: Duration
    ) async throws -> NativeJSONValue {
        try await request(
            method: "tools/call",
            params: .object(["name": .string(name), "arguments": arguments]),
            timeout: timeout,
            timeoutBehavior: .cancelRequest
        )
    }

    func terminate() {
        stop(with: NativePluginRuntimeError.cancelled, terminateProcess: true)
    }

    private func request(
        method: String,
        params: NativeJSONValue,
        timeout: Duration,
        timeoutBehavior: TimeoutBehavior = .terminateProcess
    ) async throws -> NativeJSONValue {
        guard !stopped, process.isRunning else { throw NativePluginRuntimeError.processUnavailable }
        let requestID = nextRequestID
        nextRequestID += 1
        let envelope = NativeJSONValue.object([
            "jsonrpc": .string("2.0"),
            "id": .number(Double(requestID)),
            "method": .string(method),
            "params": params,
        ])
        let data = try encodedLine(envelope)
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                pending[requestID] = PendingRequest(
                    continuation: continuation,
                    timeoutBehavior: timeoutBehavior
                )
                do {
                    try input.write(contentsOf: data)
                    Task { [weak self] in
                        try? await Task.sleep(for: timeout)
                        await self?.timeoutRequest(requestID)
                    }
                } catch {
                    pending.removeValue(forKey: requestID)
                    continuation.resume(throwing: error)
                }
            }
        } onCancel: {
            Task { [weak self] in await self?.cancelRequest(requestID) }
        }
    }

    private func notify(method: String, params: NativeJSONValue) throws {
        guard !stopped, process.isRunning else { throw NativePluginRuntimeError.processUnavailable }
        try input.write(contentsOf: encodedLine(.object([
            "jsonrpc": .string("2.0"),
            "method": .string(method),
            "params": params,
        ])))
    }

    private func encodedLine(_ value: NativeJSONValue) throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.withoutEscapingSlashes]
        var data = try encoder.encode(value)
        data.append(0x0A)
        return data
    }

    private func consume(_ data: Data) {
        guard !stopped else { return }
        readBuffer.append(data)
        while let newline = readBuffer.firstIndex(of: 0x0A) {
            let line = readBuffer[..<newline]
            readBuffer.removeSubrange(...newline)
            guard !line.isEmpty else { continue }
            guard line.count <= maximumMessageBytes else {
                stop(with: NativePluginRuntimeError.invalidMCPResponse("Plugin MCP 响应超过大小限制"))
                return
            }
            let value: NativeJSONValue
            do {
                value = try JSONDecoder().decode(NativeJSONValue.self, from: Data(line))
            } catch {
                stop(with: NativePluginRuntimeError.invalidMCPResponse(
                    "Plugin MCP 返回了无法解析的 JSON 响应"
                ))
                return
            }
            guard let object = value.jsonObject,
                  let idNumber = object["id"]?.jsonNumber else {
                // Valid server notifications do not have a request id.
                continue
            }
            let id = Int(idNumber)
            guard let request = pending.removeValue(forKey: id) else { continue }
            if let error = object["error"]?.jsonObject {
                let message = error["message"]?.jsonString ?? "Plugin MCP 调用失败"
                request.continuation.resume(throwing: NativePluginRuntimeError.mcpError(message))
            } else {
                request.continuation.resume(returning: object["result"] ?? .null)
            }
        }
        guard readBuffer.count <= maximumMessageBytes else {
            stop(with: NativePluginRuntimeError.invalidMCPResponse("Plugin MCP 响应超过大小限制"))
            return
        }
    }

    private func consumeError(_ data: Data) {
        guard !stopped, !data.isEmpty else { return }
        errorBuffer.append(data)
        if errorBuffer.count > maximumErrorBytes {
            errorBuffer.removeFirst(errorBuffer.count - maximumErrorBytes)
        }
    }

    private func timeoutRequest(_ requestID: Int) {
        guard let request = pending[requestID] else { return }
        if request.timeoutBehavior == .cancelRequest {
            pending.removeValue(forKey: requestID)
            sendCancellationNotification(requestID: requestID)
            request.continuation.resume(throwing: NativePluginRuntimeError.timeout)
            return
        }
        // stdio JSON-RPC has no portable way to interrupt an arbitrary plugin
        // call. Merely timing out the continuation leaves the child blocked
        // and every later request queues behind it. Treat a hard timeout as a
        // failed process session and terminate the child so it cannot leave a
        // Task Runner run looking active for hours.
        logPluginDiagnostics(reason: "request \(requestID) timed out")
        stop(with: NativePluginRuntimeError.timeout, terminateProcess: true)
    }

    private func cancelRequest(_ requestID: Int) {
        guard let request = pending[requestID] else { return }
        if request.timeoutBehavior == .cancelRequest {
            pending.removeValue(forKey: requestID)
            sendCancellationNotification(requestID: requestID)
            request.continuation.resume(throwing: NativePluginRuntimeError.cancelled)
            return
        }
        // MCP stdio does not provide a reliable, universally supported way to
        // interrupt one in-flight tools/call request. Removing only the Swift
        // continuation would leave the child blocked and make every later
        // request queue behind it. Cancellation therefore invalidates this
        // process session just like a hard timeout.
        stop(with: NativePluginRuntimeError.cancelled, terminateProcess: true)
    }

    private func sendCancellationNotification(requestID: Int) {
        try? notify(
            method: "notifications/cancelled",
            params: .object(["requestId": .number(Double(requestID))])
        )
    }

    private func processEnded(exitCode: Int32) {
        if exitCode != 0 {
            logPluginDiagnostics(reason: "process exited with code \(exitCode)")
        }
        stop(with: NativePluginRuntimeError.processExited(exitCode))
    }

    private func logPluginDiagnostics(reason: String) {
        let tail = Self.redactedErrorTail(errorBuffer)
        guard !tail.isEmpty else {
            Self.logger.error("Plugin MCP \(reason, privacy: .public); stderr was empty")
            return
        }
        Self.logger.error(
            "Plugin MCP \(reason, privacy: .public); redacted stderr tail: \(tail, privacy: .public)"
        )
    }

    private static func redactedErrorTail(_ data: Data) -> String {
        var value = String(decoding: data, as: UTF8.self)
            .replacingOccurrences(of: "[\\r\\n\\t]+", with: " ", options: .regularExpression)
            .replacingOccurrences(of: "[\\x00-\\x1F\\x7F]", with: "", options: .regularExpression)
        let sensitivePatterns = [
            (
                "(?i)(authorization|password|passwd|secret|token|cookie)(\\s*[:=]\\s*)([^,; ]+)",
                "$1$2<redacted>"
            ),
            ("(?i)(bearer\\s+)[A-Za-z0-9._~+/=-]+", "$1<redacted>"),
        ]
        for (pattern, replacement) in sensitivePatterns {
            value = value.replacingOccurrences(
                of: pattern,
                with: replacement,
                options: .regularExpression
            )
        }
        return String(value.trimmingCharacters(in: .whitespacesAndNewlines).suffix(2_000))
    }

    private func stop(with error: any Error, terminateProcess: Bool = false) {
        guard !stopped else { return }
        stopped = true
        output.readabilityHandler = nil
        errorOutput.readabilityHandler = nil
        outputReaderTask?.cancel()
        errorReaderTask?.cancel()
        outputReaderTask = nil
        errorReaderTask = nil
        process.terminationHandler = nil
        output.closeFile()
        errorOutput.closeFile()
        input.closeFile()
        if terminateProcess, process.isRunning {
            process.terminate()
        }
        let requests = pending.values
        pending.removeAll()
        for request in requests {
            request.continuation.resume(throwing: error)
        }
    }
}

enum NativePluginRuntimeError: LocalizedError {
    case invalidRequest(String)
    case invalidManifest(String)
    case permissionDenied(String)
    case invalidMCPResponse(String)
    case mcpError(String)
    case sessionNotFound
    case processUnavailable
    case processExited(Int32)
    case timeout
    case cancelled

    var errorDescription: String? {
        switch self {
        case let .invalidRequest(message), let .invalidManifest(message),
             let .permissionDenied(message), let .invalidMCPResponse(message),
             let .mcpError(message): message
        case .sessionNotFound: "Plugin 本机会话不存在或已经结束"
        case .processUnavailable: "Plugin MCP 进程不可用"
        case let .processExited(code): "Plugin MCP 进程已退出（\(code)）"
        case .timeout: "Plugin MCP 调用超时"
        case .cancelled: "Plugin MCP 调用已取消"
        }
    }
}

extension NativeJSONValue {
    var jsonObject: [String: NativeJSONValue]? {
        guard case let .object(value) = self else { return nil }
        return value
    }

    var jsonArray: [NativeJSONValue]? {
        guard case let .array(value) = self else { return nil }
        return value
    }

    var jsonString: String? {
        guard case let .string(value) = self else { return nil }
        return value
    }

    var jsonNumber: Double? {
        guard case let .number(value) = self else { return nil }
        return value
    }

    var jsonBool: Bool? {
        guard case let .bool(value) = self else { return nil }
        return value
    }
}
