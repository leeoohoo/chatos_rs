import ChatOSMediaWorkerCore
import ChatOSMediaWorkerProtocol
import Darwin
import Dispatch
import Foundation

private enum WorkerExit: Int32 {
    case usage = 64
    case protocolFailure = 65
    case parentExited = 70
    case ioFailure = 74
}

private struct WorkerFailure: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

private func readLine(descriptor: Int32) throws -> Data {
    var result = Data()
    var buffer = [UInt8](repeating: 0, count: 4_096)
    while result.count <= ChatOSMediaWorkerProtocol.maximumInlineBytes {
        let count = Darwin.read(descriptor, &buffer, buffer.count)
        if count == 0 { throw WorkerFailure(message: "Unexpected end of inherited input.") }
        if count < 0, errno == EINTR { continue }
        guard count > 0 else { throw WorkerFailure(message: "Inherited input could not be read.") }
        if let newline = buffer[..<count].firstIndex(of: 0x0A) {
            result.append(contentsOf: buffer[..<newline])
            return result
        }
        result.append(contentsOf: buffer[..<count])
    }
    throw WorkerFailure(message: "IPC message exceeded the inline byte limit.")
}

private func writeLine<T: Encodable>(_ value: T) throws {
    let data = try MediaWorkerWireCodec.encodeLine(value)
    try data.withUnsafeBytes { rawBuffer in
        guard let baseAddress = rawBuffer.baseAddress else { return }
        var offset = 0
        while offset < rawBuffer.count {
            let count = Darwin.write(
                STDOUT_FILENO,
                baseAddress.advanced(by: offset),
                rawBuffer.count - offset
            )
            if count > 0 {
                offset += count
            } else if count < 0, errno == EINTR {
                continue
            } else {
                throw WorkerFailure(message: "Inherited output could not be written.")
            }
        }
    }
}

private func terminate(_ exit: WorkerExit) -> Never {
    Darwin._exit(exit.rawValue)
}

guard CommandLine.arguments.count == 3,
      CommandLine.arguments[1] == "--parent-pid",
      let parentProcessID = Int32(CommandLine.arguments[2]),
      parentProcessID > 1,
      getppid() == parentProcessID else {
    terminate(.usage)
}

_ = signal(SIGPIPE, SIG_IGN)
let parentMonitor = DispatchSource.makeProcessSource(
    identifier: parentProcessID,
    eventMask: .exit,
    queue: DispatchQueue.global(qos: .userInitiated)
)
parentMonitor.setEventHandler { terminate(.parentExited) }
parentMonitor.resume()
guard kill(parentProcessID, 0) == 0 || errno == EPERM else {
    terminate(.parentExited)
}

let helperInstanceID = UUID().uuidString.lowercased()
do {
    let hello = try MediaWorkerWireCodec.decode(
        MediaWorkerHelloRequest.self,
        from: readLine(descriptor: STDIN_FILENO)
    )
    guard hello.type == "hello",
          hello.parentProcessID == parentProcessID,
          hello.maximumInlineBytes == ChatOSMediaWorkerProtocol.maximumInlineBytes,
          hello.protocolMinVersion <= ChatOSMediaWorkerProtocol.maximumVersion,
          hello.protocolMaxVersion >= ChatOSMediaWorkerProtocol.minimumVersion,
          hello.capabilities.contains(ChatOSMediaWorkerProtocol.normalizePNGOperation) else {
        try writeLine(MediaWorkerHelloResponse(
            ok: false,
            selectedProtocolVersion: nil,
            helperInstanceID: helperInstanceID,
            error: .init(code: "incompatible_protocol", message: "No compatible media worker protocol.")
        ))
        terminate(.protocolFailure)
    }
    try writeLine(MediaWorkerHelloResponse(
        ok: true,
        selectedProtocolVersion: ChatOSMediaWorkerProtocol.maximumVersion,
        helperInstanceID: helperInstanceID
    ))

    let request = try MediaWorkerWireCodec.decode(
        MediaWorkerNormalizePNGRequest.self,
        from: readLine(descriptor: STDIN_FILENO)
    )
    guard request.helperInstanceID == helperInstanceID else {
        throw MediaWorkerCoreError.invalidRequest("The helper instance does not match the handshake.")
    }
    do {
        let result = try ChatOSMediaWorkerEngine.normalizePNG(request)
        try writeLine(MediaWorkerNormalizePNGResponse(
            requestID: request.requestID,
            result: result
        ))
    } catch let error as MediaWorkerCoreError {
        try writeLine(MediaWorkerNormalizePNGResponse(
            requestID: request.requestID,
            error: error.payload,
            retryable: false
        ))
    } catch {
        try writeLine(MediaWorkerNormalizePNGResponse(
            requestID: request.requestID,
            error: .init(code: "worker_failure", message: "The media worker could not normalize the image."),
            retryable: false
        ))
    }
    parentMonitor.cancel()
    Darwin._exit(EXIT_SUCCESS)
} catch {
    parentMonitor.cancel()
    let message = "ChatOSMediaWorker transport failure: \(String(describing: error).prefix(512))\n"
    try? FileHandle.standardError.write(contentsOf: Data(message.utf8))
    terminate(.ioFailure)
}
