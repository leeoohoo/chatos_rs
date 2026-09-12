// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Darwin
import Foundation

public let localAgentHostLaunchProtocolVersion: UInt32 = 1

public enum NativeLocalAgentHostLaunchError: Error, Equatable, Sendable {
    case invalidConfiguration(String)
    case untrustedExecutable
    case processLaunchFailed(String)
    case launchFrameWriteFailed
    case readyTimeout
    case readyFrameInvalid
    case readyProtocolMismatch(UInt32)
    case readyLaunchMismatch
    case readyProcessMismatch
    case readyEndpointMismatch
}

extension NativeLocalAgentHostLaunchError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case let .invalidConfiguration(message): message
        case .untrustedExecutable: "本地 Agent Host 可执行文件未通过身份校验"
        case let .processLaunchFailed(message): "本地 Agent Host 启动失败：\(message)"
        case .launchFrameWriteFailed: "本地 Agent Host 启动凭据写入失败"
        case .readyTimeout: "本地 Agent Host 启动握手超时"
        case .readyFrameInvalid: "本地 Agent Host 返回了无效启动握手"
        case let .readyProtocolMismatch(version): "本地 Agent Host 启动协议不匹配（\(version)）"
        case .readyLaunchMismatch: "本地 Agent Host 启动标识不匹配"
        case .readyProcessMismatch: "本地 Agent Host 进程身份不匹配"
        case .readyEndpointMismatch: "本地 Agent Host IPC 地址不匹配"
        }
    }
}

public struct NativeLocalAgentHostLaunchConfiguration: Sendable {
    public let executableURL: URL
    public let launchID: String
    public let expectedClientEndpoint: String
    public let launchMaterial: NativeLocalAgentHostLaunchMaterial
    public let readyTimeout: Duration

    public init(
        executableURL: URL,
        launchID: String,
        expectedClientEndpoint: String,
        launchRequestJSON: Data,
        readyTimeout: Duration = .seconds(30)
    ) throws {
        guard executableURL.isFileURL,
              executableURL.path.hasPrefix("/"),
              !launchID.isEmpty,
              launchID == launchID.trimmingCharacters(in: .whitespacesAndNewlines),
              !expectedClientEndpoint.isEmpty,
              expectedClientEndpoint == expectedClientEndpoint.trimmingCharacters(in: .whitespacesAndNewlines),
              !launchRequestJSON.isEmpty,
              launchRequestJSON.count <= 1024 * 1024,
              readyTimeout > .zero,
              readyTimeout <= .seconds(120)
        else {
            throw NativeLocalAgentHostLaunchError.invalidConfiguration(
                "本地 Agent Host 启动配置无效"
            )
        }
        self.executableURL = executableURL
        self.launchID = launchID
        self.expectedClientEndpoint = expectedClientEndpoint
        self.launchMaterial = try NativeLocalAgentHostLaunchMaterial(launchRequestJSON)
        self.readyTimeout = readyTimeout
    }
}

public final class NativeLocalAgentHostLaunchMaterial: @unchecked Sendable {
    private let lock = NSLock()
    private var requestJSON: Data?

    public init(_ requestJSON: Data) throws {
        guard !requestJSON.isEmpty, requestJSON.count <= 1024 * 1024 else {
            throw NativeLocalAgentHostLaunchError.invalidConfiguration(
                "本地 Agent Host 启动凭据大小无效"
            )
        }
        self.requestJSON = requestJSON
    }

    deinit {
        lock.lock()
        zeroize(&requestJSON)
        lock.unlock()
    }

    fileprivate func consume() throws -> Data {
        lock.lock()
        defer { lock.unlock() }
        guard let data = requestJSON else {
            throw NativeLocalAgentHostLaunchError.invalidConfiguration(
                "本地 Agent Host 启动凭据已经使用"
            )
        }
        requestJSON = nil
        return data
    }

    func snapshotForTesting() throws -> Data {
        lock.lock()
        defer { lock.unlock() }
        guard let requestJSON else {
            throw NativeLocalAgentHostLaunchError.invalidConfiguration(
                "本地 Agent Host 启动凭据已经使用"
            )
        }
        return requestJSON
    }

    private func zeroize(_ value: inout Data?) {
        let count = value?.count ?? 0
        value?.resetBytes(in: 0..<count)
        value = nil
    }
}

public struct NativeLocalAgentHostReady: Decodable, Equatable, Sendable {
    public let protocolVersion: UInt32
    public let launchID: String
    public let processID: UInt32
    public let clientEndpoint: String

    private enum CodingKeys: String, CodingKey {
        case protocolVersion = "protocol_version"
        case launchID = "launch_id"
        case processID = "process_id"
        case clientEndpoint = "client_endpoint"
    }
}

public final class NativeLocalAgentHostProcess: @unchecked Sendable {
    public let ready: NativeLocalAgentHostReady
    private let process: Process

    fileprivate init(process: Process, ready: NativeLocalAgentHostReady) {
        self.process = process
        self.ready = ready
    }

    public var isRunning: Bool { process.isRunning }

    public func terminate() {
        guard process.isRunning else { return }
        process.terminate()
    }

    public func waitForExit() async -> Int32 {
        await Task.detached(priority: .utility) { [process] in
            process.waitUntilExit()
            return process.terminationStatus
        }.value
    }
}

/// Launches the bundled Rust Host without placing any credential in argv,
/// environment variables, preferences, or a temporary file. The one-time
/// secret frame is written to stdin and closed immediately; stdout is accepted
/// only as the correlated, non-secret ready frame.
public struct NativeLocalAgentHostProcessLauncher: Sendable {
    public init() {}

    public func launch(
        _ configuration: NativeLocalAgentHostLaunchConfiguration
    ) async throws -> NativeLocalAgentHostProcess {
        try verifyExecutable(configuration.executableURL)
        let process = Process()
        let input = Pipe()
        let output = Pipe()
        process.executableURL = configuration.executableURL
        process.arguments = []
        process.environment = ["LANG": "en_US.UTF-8"]
        process.currentDirectoryURL = URL(fileURLWithPath: "/", isDirectory: true)
        process.standardInput = input
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            throw NativeLocalAgentHostLaunchError.processLaunchFailed(error.localizedDescription)
        }

        do {
            var launchRequestJSON = try configuration.launchMaterial.consume()
            defer { launchRequestJSON.resetBytes(in: 0..<launchRequestJSON.count) }
            try writeLaunchFrame(
                launchRequestJSON,
                to: input.fileHandleForWriting
            )
            let ready = try await readReadyFrame(
                from: output.fileHandleForReading,
                timeout: configuration.readyTimeout
            )
            try validateReady(ready, configuration: configuration, process: process)
            return NativeLocalAgentHostProcess(process: process, ready: ready)
        } catch {
            input.fileHandleForWriting.closeFile()
            output.fileHandleForReading.closeFile()
            if process.isRunning { process.terminate() }
            _ = await Task.detached(priority: .utility) {
                process.waitUntilExit()
            }.value
            throw error
        }
    }

    private func verifyExecutable(_ url: URL) throws {
        var metadata = stat()
        guard lstat(url.path, &metadata) == 0,
              metadata.st_mode & S_IFMT == S_IFREG,
              metadata.st_uid == 0 || metadata.st_uid == geteuid(),
              metadata.st_mode & 0o022 == 0,
              access(url.path, X_OK) == 0
        else {
            throw NativeLocalAgentHostLaunchError.untrustedExecutable
        }
    }

    private func writeLaunchFrame(_ body: Data, to handle: FileHandle) throws {
        var length = UInt32(body.count).bigEndian
        do {
            try withUnsafeBytes(of: &length) { bytes in
                try handle.write(contentsOf: Data(bytes))
            }
            try handle.write(contentsOf: body)
            try handle.close()
        } catch {
            throw NativeLocalAgentHostLaunchError.launchFrameWriteFailed
        }
    }

    private func readReadyFrame(
        from handle: FileHandle,
        timeout: Duration
    ) async throws -> NativeLocalAgentHostReady {
        let reader = BlockingReadyReader(handle: handle)
        return try await withThrowingTaskGroup(of: NativeLocalAgentHostReady.self) { group in
            group.addTask { try reader.read() }
            group.addTask {
                try await Task.sleep(for: timeout)
                reader.timeout()
                throw NativeLocalAgentHostLaunchError.readyTimeout
            }
            guard let first = try await group.next() else {
                throw NativeLocalAgentHostLaunchError.readyFrameInvalid
            }
            group.cancelAll()
            reader.close()
            return first
        }
    }

    private func validateReady(
        _ ready: NativeLocalAgentHostReady,
        configuration: NativeLocalAgentHostLaunchConfiguration,
        process: Process
    ) throws {
        guard ready.protocolVersion == localAgentHostLaunchProtocolVersion else {
            throw NativeLocalAgentHostLaunchError.readyProtocolMismatch(ready.protocolVersion)
        }
        guard ready.launchID == configuration.launchID else {
            throw NativeLocalAgentHostLaunchError.readyLaunchMismatch
        }
        guard ready.processID == UInt32(process.processIdentifier) else {
            throw NativeLocalAgentHostLaunchError.readyProcessMismatch
        }
        guard ready.clientEndpoint == configuration.expectedClientEndpoint else {
            throw NativeLocalAgentHostLaunchError.readyEndpointMismatch
        }
    }
}

private final class BlockingReadyReader: @unchecked Sendable {
    private let handle: FileHandle
    private let lock = NSLock()
    private var closed = false
    private var timedOut = false

    init(handle: FileHandle) {
        self.handle = handle
    }

    func read() throws -> NativeLocalAgentHostReady {
        let lengthData = try readExactly(4)
        let length = lengthData.reduce(UInt32(0)) { value, byte in
            (value << 8) | UInt32(byte)
        }
        guard length > 0, length <= 1024 * 1024 else {
            throw NativeLocalAgentHostLaunchError.readyFrameInvalid
        }
        let body = try readExactly(Int(length))
        guard let ready = try? JSONDecoder().decode(NativeLocalAgentHostReady.self, from: body)
        else {
            throw NativeLocalAgentHostLaunchError.readyFrameInvalid
        }
        return ready
    }

    func close() {
        lock.lock()
        defer { lock.unlock() }
        guard !closed else { return }
        closed = true
        try? handle.close()
    }

    func timeout() {
        lock.lock()
        timedOut = true
        guard !closed else {
            lock.unlock()
            return
        }
        closed = true
        lock.unlock()
        try? handle.close()
    }

    private func readExactly(_ count: Int) throws -> Data {
        var data = Data()
        while data.count < count {
            let chunk: Data?
            do {
                chunk = try handle.read(upToCount: count - data.count)
            } catch {
                throw readFailure()
            }
            guard let chunk, !chunk.isEmpty else {
                throw readFailure()
            }
            data.append(chunk)
        }
        return data
    }

    private func readFailure() -> NativeLocalAgentHostLaunchError {
        lock.lock()
        defer { lock.unlock() }
        return timedOut ? .readyTimeout : .readyFrameInvalid
    }
}
