import ChatOSCodeReadWorkerCore
import ChatOSCodeReadWorkerProtocol
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
    var byte: UInt8 = 0
    while result.count <= ChatOSCodeReadWorkerProtocol.maximumInlineBytes {
        let count = Darwin.read(descriptor, &byte, 1)
        if count == 1 {
            if byte == 0x0A { return result }
            result.append(byte)
        } else if count < 0, errno == EINTR {
            continue
        } else {
            throw WorkerFailure(message: "Inherited input could not be read.")
        }
    }
    throw WorkerFailure(message: "IPC message exceeded the inline byte limit.")
}

private func writeLine<T: Encodable>(_ value: T) throws {
    let data = try CodeReadWorkerWireCodec.encodeLine(value)
    try data.withUnsafeBytes { rawBuffer in
        guard let baseAddress = rawBuffer.baseAddress else { return }
        var offset = 0
        while offset < rawBuffer.count {
            let count = Darwin.write(
                STDOUT_FILENO,
                baseAddress.advanced(by: offset),
                rawBuffer.count - offset
            )
            if count > 0 { offset += count }
            else if count < 0, errno == EINTR { continue }
            else { throw WorkerFailure(message: "Inherited output could not be written.") }
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
      getppid() == parentProcessID,
      setpgid(0, 0) == 0 else {
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
    let hello = try CodeReadWorkerWireCodec.decode(
        CodeReadWorkerHelloRequest.self,
        from: readLine(descriptor: STDIN_FILENO)
    )
    guard hello.type == "hello",
          hello.parentProcessID == parentProcessID,
          hello.maximumInlineBytes == ChatOSCodeReadWorkerProtocol.maximumInlineBytes,
          hello.protocolMinVersion <= ChatOSCodeReadWorkerProtocol.maximumVersion,
          hello.protocolMaxVersion >= ChatOSCodeReadWorkerProtocol.minimumVersion else {
        try writeLine(CodeReadWorkerHelloResponse(
            ok: false,
            selectedProtocolVersion: nil,
            helperInstanceID: helperInstanceID,
            error: .init(code: "incompatible_protocol", message: "No compatible code-read worker protocol.")
        ))
        terminate(.protocolFailure)
    }
    try writeLine(CodeReadWorkerHelloResponse(
        ok: true,
        selectedProtocolVersion: ChatOSCodeReadWorkerProtocol.maximumVersion,
        helperInstanceID: helperInstanceID
    ))

    let request = try CodeReadWorkerWireCodec.decode(
        CodeReadWorkerRequest.self,
        from: readLine(descriptor: STDIN_FILENO)
    )
    guard request.helperInstanceID == helperInstanceID else {
        throw CodeReadWorkerCoreError.invalidRequest("The helper instance does not match the handshake.")
    }
    do {
        let result = try ChatOSCodeReadWorkerEngine.execute(request)
        try writeLine(CodeReadWorkerResponse(requestID: request.requestID, result: result))
    } catch let error as CodeReadWorkerCoreError {
        try writeLine(CodeReadWorkerResponse(
            requestID: request.requestID,
            error: error.payload,
            retryable: false
        ))
    } catch {
        try writeLine(CodeReadWorkerResponse(
            requestID: request.requestID,
            error: .init(code: "worker_failure", message: "代码读取独立进程执行失败"),
            retryable: false
        ))
    }
    parentMonitor.cancel()
    Darwin._exit(EXIT_SUCCESS)
} catch {
    parentMonitor.cancel()
    let message = "ChatOSCodeReadWorker transport failure: \(String(describing: error).prefix(512))\n"
    try? FileHandle.standardError.write(contentsOf: Data(message.utf8))
    terminate(.ioFailure)
}
