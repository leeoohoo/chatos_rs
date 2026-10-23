import ChatOSMediaWorkerProtocol
import Darwin
import Foundation

protocol MediaWorkerImageNormalizing: Sendable {
    func normalizePNG(
        sourceURL: URL,
        sourceIdentity: MediaWorkerFileIdentity?,
        destinationURL: URL,
        ownerUserID: String,
        maximumSourceBytes: Int,
        maximumSourcePixelCount: Int,
        maximumOutputPixelSize: Int,
        maximumOutputBytes: Int
    ) async throws -> MediaWorkerImageDescriptor
}

protocol MediaWorkerVideoFrameExtracting: Sendable {
    func extractLastVideoFramePNG(
        sourceURL: URL,
        destinationURL: URL,
        ownerUserID: String,
        maximumSourceBytes: Int,
        maximumOutputPixelSize: Int,
        maximumOutputBytes: Int
    ) async throws -> MediaWorkerImageDescriptor
}

enum MediaWorkerProcessClientError: LocalizedError, Equatable {
    case unavailable
    case invalidExecutable
    case launchFailed
    case incompatibleProtocol
    case invalidResponse
    case requestRejected(code: String)
    case timedOut
    case quarantined

    var errorDescription: String? {
        switch self {
        case .unavailable:
            "图片处理进程不可用。"
        case .invalidExecutable:
            "图片处理进程未通过本机文件校验。"
        case .launchFailed:
            "无法启动图片处理进程。"
        case .incompatibleProtocol:
            "图片处理进程版本与客户端不兼容。"
        case .invalidResponse:
            "图片处理进程返回了无效结果。"
        case let .requestRejected(code):
            "图片处理失败（\(code)）。"
        case .timedOut:
            "图片处理超时，已终止独立处理进程。"
        case .quarantined:
            "图片处理进程连续异常，已暂时停用。"
        }
    }
}

struct MediaWorkerProcessClient: MediaWorkerImageNormalizing, MediaWorkerVideoFrameExtracting {
    static let shared = MediaWorkerProcessClient()

    private static let accountSessionID = UUID().uuidString.lowercased()
    private static let sessionEpoch = UUID().uuidString.lowercased()
    private static let circuitBreaker = MediaWorkerCircuitBreaker()

    let executableURL: URL?
    let timeout: TimeInterval

    init(executableURL: URL? = nil, timeout: TimeInterval = 60) {
        self.executableURL = executableURL
        self.timeout = timeout
    }

    func normalizePNG(
        sourceURL: URL,
        sourceIdentity: MediaWorkerFileIdentity? = nil,
        destinationURL: URL,
        ownerUserID: String,
        maximumSourceBytes: Int,
        maximumSourcePixelCount: Int,
        maximumOutputPixelSize: Int,
        maximumOutputBytes: Int
    ) async throws -> MediaWorkerImageDescriptor {
        guard timeout.isFinite,
              (0.05...300).contains(timeout),
              !ownerUserID.isEmpty,
              ownerUserID.utf8.count <= 512 else {
            throw MediaWorkerProcessClientError.invalidResponse
        }
        if let delay = try await Self.circuitBreaker.admissionDelay() {
            try await Task.sleep(for: delay)
        }
        let resolvedExecutable = try executableURL ?? Self.resolveExecutable()
        let controller = MediaWorkerProcessController()
        do {
            let result = try await withTaskCancellationHandler {
                try await withThrowingTaskGroup(of: MediaWorkerImageDescriptor.self) { group in
                    group.addTask {
                        try await withTaskCancellationHandler {
                            try await Task.detached(priority: .userInitiated) {
                                try controller.run(
                                    executableURL: resolvedExecutable,
                                    sourceURL: sourceURL,
                                    authorizedSourceIdentity: sourceIdentity,
                                    destinationURL: destinationURL,
                                    ownerUserID: ownerUserID,
                                    maximumSourceBytes: maximumSourceBytes,
                                    maximumSourcePixelCount: maximumSourcePixelCount,
                                    maximumOutputPixelSize: maximumOutputPixelSize,
                                    maximumOutputBytes: maximumOutputBytes,
                                    operation: .normalizePNG,
                                    timeout: timeout,
                                    accountSessionID: Self.accountSessionID,
                                    sessionEpoch: Self.sessionEpoch
                                )
                            }.value
                        } onCancel: {
                            controller.terminate()
                        }
                    }
                    group.addTask {
                        let nanoseconds = UInt64(max(0.001, timeout) * 1_000_000_000)
                        try await Task.sleep(nanoseconds: nanoseconds)
                        controller.terminate()
                        throw MediaWorkerProcessClientError.timedOut
                    }
                    defer { group.cancelAll() }
                    guard let first = try await group.next() else {
                        throw MediaWorkerProcessClientError.invalidResponse
                    }
                    return first
                }
            } onCancel: {
                controller.terminate()
            }
            await Self.circuitBreaker.recordSuccess()
            return result
        } catch is CancellationError {
            throw CancellationError()
        } catch let error as MediaWorkerProcessClientError {
            if Task.isCancelled { throw CancellationError() }
            if case .requestRejected = error {
                throw error
            }
            await Self.circuitBreaker.recordAbnormalFailure()
            throw error
        } catch {
            await Self.circuitBreaker.recordAbnormalFailure()
            throw MediaWorkerProcessClientError.invalidResponse
        }
    }

    func extractLastVideoFramePNG(
        sourceURL: URL,
        destinationURL: URL,
        ownerUserID: String,
        maximumSourceBytes: Int,
        maximumOutputPixelSize: Int,
        maximumOutputBytes: Int
    ) async throws -> MediaWorkerImageDescriptor {
        guard timeout.isFinite,
              (0.05...300).contains(timeout),
              !ownerUserID.isEmpty,
              ownerUserID.utf8.count <= 512 else {
            throw MediaWorkerProcessClientError.invalidResponse
        }
        if let delay = try await Self.circuitBreaker.admissionDelay() {
            try await Task.sleep(for: delay)
        }
        let resolvedExecutable = try executableURL ?? Self.resolveExecutable()
        let controller = MediaWorkerProcessController()
        do {
            let result = try await withTaskCancellationHandler {
                try await withThrowingTaskGroup(of: MediaWorkerImageDescriptor.self) { group in
                    group.addTask {
                        try await withTaskCancellationHandler {
                            try await Task.detached(priority: .userInitiated) {
                                try controller.run(
                                    executableURL: resolvedExecutable,
                                    sourceURL: sourceURL,
                                    authorizedSourceIdentity: nil,
                                    destinationURL: destinationURL,
                                    ownerUserID: ownerUserID,
                                    maximumSourceBytes: maximumSourceBytes,
                                    maximumSourcePixelCount: 1,
                                    maximumOutputPixelSize: maximumOutputPixelSize,
                                    maximumOutputBytes: maximumOutputBytes,
                                    operation: .extractLastVideoFramePNG,
                                    timeout: timeout,
                                    accountSessionID: Self.accountSessionID,
                                    sessionEpoch: Self.sessionEpoch
                                )
                            }.value
                        } onCancel: {
                            controller.terminate()
                        }
                    }
                    group.addTask {
                        let nanoseconds = UInt64(max(0.001, timeout) * 1_000_000_000)
                        try await Task.sleep(nanoseconds: nanoseconds)
                        controller.terminate()
                        throw MediaWorkerProcessClientError.timedOut
                    }
                    defer { group.cancelAll() }
                    guard let first = try await group.next() else {
                        throw MediaWorkerProcessClientError.invalidResponse
                    }
                    return first
                }
            } onCancel: {
                controller.terminate()
            }
            await Self.circuitBreaker.recordSuccess()
            return result
        } catch is CancellationError {
            throw CancellationError()
        } catch let error as MediaWorkerProcessClientError {
            if Task.isCancelled { throw CancellationError() }
            if case .requestRejected = error { throw error }
            await Self.circuitBreaker.recordAbnormalFailure()
            throw error
        } catch {
            await Self.circuitBreaker.recordAbnormalFailure()
            throw MediaWorkerProcessClientError.invalidResponse
        }
    }

    private static func resolveExecutable() throws -> URL {
        #if DEBUG
        if let configured = ProcessInfo.processInfo.environment["CHATOS_MEDIA_WORKER_PATH"]?
            .trimmingCharacters(in: .whitespacesAndNewlines),
           !configured.isEmpty {
            return URL(fileURLWithPath: configured)
        }
        #endif
        if let bundled = Bundle.main.url(forAuxiliaryExecutable: "ChatOSMediaWorker") {
            return bundled
        }
        #if DEBUG
        if let executable = Bundle.main.executableURL {
            let sibling = executable.deletingLastPathComponent()
                .appendingPathComponent("ChatOSMediaWorker", isDirectory: false)
            if FileManager.default.isExecutableFile(atPath: sibling.path) {
                return sibling
            }
        }
        #endif
        throw MediaWorkerProcessClientError.unavailable
    }
}

private enum MediaWorkerProcessOperation {
    case normalizePNG
    case extractLastVideoFramePNG

    var capability: String {
        switch self {
        case .normalizePNG: ChatOSMediaWorkerProtocol.normalizePNGOperation
        case .extractLastVideoFramePNG:
            ChatOSMediaWorkerProtocol.extractLastVideoFramePNGOperation
        }
    }
}

private actor MediaWorkerCircuitBreaker {
    private var abnormalFailureDates: [ContinuousClock.Instant] = []
    private var quarantineUntil: ContinuousClock.Instant?

    func admissionDelay() throws -> Duration? {
        let now = ContinuousClock.now
        abnormalFailureDates.removeAll {
            now - $0 > .seconds(300)
        }
        if let quarantineUntil, quarantineUntil > now {
            throw MediaWorkerProcessClientError.quarantined
        }
        self.quarantineUntil = nil
        guard !abnormalFailureDates.isEmpty else { return nil }
        let seconds = min(30, 1 << min(abnormalFailureDates.count - 1, 5))
        return .seconds(seconds)
    }

    func recordSuccess() {
        abnormalFailureDates.removeAll()
        quarantineUntil = nil
    }

    func recordAbnormalFailure() {
        let now = ContinuousClock.now
        abnormalFailureDates.removeAll {
            now - $0 > .seconds(300)
        }
        abnormalFailureDates.append(now)
        if abnormalFailureDates.count >= 5 {
            quarantineUntil = now.advanced(by: .seconds(30))
        }
    }
}

private final class MediaWorkerProcessController: @unchecked Sendable {
    private let lock = NSLock()
    private var process: Process?
    private var cancellationRequested = false

    func run(
        executableURL: URL,
        sourceURL: URL,
        authorizedSourceIdentity: MediaWorkerFileIdentity?,
        destinationURL: URL,
        ownerUserID: String,
        maximumSourceBytes: Int,
        maximumSourcePixelCount: Int,
        maximumOutputPixelSize: Int,
        maximumOutputBytes: Int,
        operation: MediaWorkerProcessOperation,
        timeout: TimeInterval,
        accountSessionID: String,
        sessionEpoch: String
    ) throws -> MediaWorkerImageDescriptor {
        try validateExecutable(executableURL)
        let currentSourceIdentity = try fileIdentity(sourceURL, requireEmpty: false)
        if let authorizedSourceIdentity,
           authorizedSourceIdentity != currentSourceIdentity {
            throw MediaWorkerProcessClientError.invalidResponse
        }
        let sourceIdentity = authorizedSourceIdentity ?? currentSourceIdentity
        let destinationIdentity = try fileIdentity(destinationURL, requireEmpty: true)
        let child = Process()
        let input = Pipe()
        let output = Pipe()
        child.executableURL = executableURL.standardizedFileURL
        child.currentDirectoryURL = executableURL.deletingLastPathComponent()
        child.arguments = ["--parent-pid", String(getpid())]
        child.environment = [
            "LANG": "en_US.UTF-8",
            "TMPDIR": FileManager.default.temporaryDirectory.path,
        ]
        child.standardInput = input
        child.standardOutput = output
        child.standardError = FileHandle.nullDevice

        do {
            try child.run()
        } catch {
            throw MediaWorkerProcessClientError.launchFailed
        }
        register(child)
        defer {
            try? input.fileHandleForWriting.close()
            try? output.fileHandleForReading.close()
            if child.isRunning { terminate() }
            clear(child)
        }

        let clientInstanceID = UUID().uuidString.lowercased()
        try input.fileHandleForWriting.write(contentsOf: MediaWorkerWireCodec.encodeLine(
            MediaWorkerHelloRequest(
                clientInstanceID: clientInstanceID,
                parentProcessID: getpid()
            )
        ))
        let hello = try MediaWorkerWireCodec.decode(
            MediaWorkerHelloResponse.self,
            from: readLine(output.fileHandleForReading)
        )
        guard hello.type == "hello_response",
              hello.ok,
              hello.selectedProtocolVersion == ChatOSMediaWorkerProtocol.maximumVersion,
              hello.workerBuildVersion == ChatOSMediaWorkerProtocol.buildVersion,
              hello.capabilities.contains(operation.capability),
              hello.maximumInlineBytes == ChatOSMediaWorkerProtocol.maximumInlineBytes,
              !hello.helperInstanceID.isEmpty else {
            throw MediaWorkerProcessClientError.incompatibleProtocol
        }

        let requestID = UUID().uuidString.lowercased()
        let deadline = Int64((Date().timeIntervalSince1970 + timeout) * 1_000)
        let requestData: Data
        switch operation {
        case .normalizePNG:
            requestData = try MediaWorkerWireCodec.encodeLine(MediaWorkerNormalizePNGRequest(
                protocolVersion: ChatOSMediaWorkerProtocol.maximumVersion,
                requestID: requestID,
                ownerUserID: ownerUserID,
                accountSessionID: accountSessionID,
                sessionEpoch: sessionEpoch,
                helperInstanceID: hello.helperInstanceID,
                deadlineUnixMilliseconds: deadline,
                sourcePath: sourceURL.path,
                sourceIdentity: sourceIdentity,
                destinationPath: destinationURL.path,
                destinationIdentity: destinationIdentity,
                maximumSourceBytes: maximumSourceBytes,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumOutputPixelSize: maximumOutputPixelSize,
                maximumOutputBytes: maximumOutputBytes
            ))
        case .extractLastVideoFramePNG:
            requestData = try MediaWorkerWireCodec.encodeLine(MediaWorkerExtractVideoFrameRequest(
                protocolVersion: ChatOSMediaWorkerProtocol.maximumVersion,
                requestID: requestID,
                ownerUserID: ownerUserID,
                accountSessionID: accountSessionID,
                sessionEpoch: sessionEpoch,
                helperInstanceID: hello.helperInstanceID,
                deadlineUnixMilliseconds: deadline,
                sourcePath: sourceURL.path,
                sourceIdentity: sourceIdentity,
                destinationPath: destinationURL.path,
                destinationIdentity: destinationIdentity,
                maximumSourceBytes: maximumSourceBytes,
                maximumOutputPixelSize: maximumOutputPixelSize,
                maximumOutputBytes: maximumOutputBytes
            ))
        }
        try input.fileHandleForWriting.write(contentsOf: requestData)
        try input.fileHandleForWriting.close()
        let response = try MediaWorkerWireCodec.decode(
            MediaWorkerNormalizePNGResponse.self,
            from: readLine(output.fileHandleForReading)
        )
        guard response.type == "response", response.requestID == requestID else {
            throw MediaWorkerProcessClientError.invalidResponse
        }
        guard response.ok, let result = response.result else {
            throw MediaWorkerProcessClientError.requestRejected(
                code: response.error?.code ?? "unknown"
            )
        }
        guard result.path == destinationURL.path,
              result.mimeType == "image/png",
              result.byteCount > 0,
              result.byteCount <= maximumOutputBytes,
              result.sha256.count == 64,
              result.pixelWidth > 0,
              result.pixelHeight > 0,
              max(result.pixelWidth, result.pixelHeight) <= maximumOutputPixelSize else {
            throw MediaWorkerProcessClientError.invalidResponse
        }

        let exitDeadline = ContinuousClock.now.advanced(by: .seconds(1))
        while child.isRunning, ContinuousClock.now < exitDeadline {
            usleep(5_000)
        }
        guard !child.isRunning, child.terminationStatus == EXIT_SUCCESS else {
            throw MediaWorkerProcessClientError.invalidResponse
        }
        return result
    }

    func terminate() {
        lock.lock()
        cancellationRequested = true
        let process = process
        lock.unlock()
        guard let process, process.isRunning else { return }
        _ = Darwin.kill(process.processIdentifier, SIGTERM)
        let processID = process.processIdentifier
        DispatchQueue.global(qos: .userInitiated).asyncAfter(deadline: .now() + 0.25) { [weak self] in
            self?.forceKill(processID)
        }
    }

    private func register(_ process: Process) {
        lock.lock()
        self.process = process
        let shouldTerminate = cancellationRequested
        lock.unlock()
        if shouldTerminate { terminate() }
    }

    private func clear(_ process: Process) {
        lock.lock()
        if self.process === process { self.process = nil }
        lock.unlock()
    }

    private func forceKill(_ processID: Int32) {
        lock.lock()
        let shouldKill = process?.processIdentifier == processID && process?.isRunning == true
        lock.unlock()
        if shouldKill { _ = Darwin.kill(processID, SIGKILL) }
    }

    private func validateExecutable(_ url: URL) throws {
        var status = stat()
        let result = url.path.withCString { Darwin.lstat($0, &status) }
        guard url.isFileURL,
              result == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              (status.st_mode & S_IXUSR) != 0 else {
            throw MediaWorkerProcessClientError.invalidExecutable
        }
    }

    private func fileIdentity(
        _ url: URL,
        requireEmpty: Bool
    ) throws -> MediaWorkerFileIdentity {
        var status = stat()
        let result = url.path.withCString { Darwin.lstat($0, &status) }
        guard url.isFileURL,
              result == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_uid == geteuid(),
              (!requireEmpty || (status.st_nlink == 1 && status.st_size == 0)) else {
            throw MediaWorkerProcessClientError.invalidResponse
        }
        return .init(
            device: UInt64(status.st_dev),
            inode: UInt64(status.st_ino),
            byteCount: Int64(status.st_size)
        )
    }

    private func readLine(_ handle: FileHandle) throws -> Data {
        var result = Data()
        var byte: UInt8 = 0
        while result.count <= ChatOSMediaWorkerProtocol.maximumInlineBytes {
            let count = Darwin.read(handle.fileDescriptor, &byte, 1)
            if count == 1 {
                if byte == 0x0A { return result }
                result.append(byte)
            } else if count < 0, errno == EINTR {
                continue
            } else {
                throw MediaWorkerProcessClientError.invalidResponse
            }
        }
        throw MediaWorkerProcessClientError.invalidResponse
    }
}
