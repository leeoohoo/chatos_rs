import Darwin
import Foundation

private enum OutputFormat {
    case ownership
    case sampling
}

private func processState(_ value: CChar) -> Character {
    switch Int32(value) {
    case SIDL: "I"
    case SRUN: "R"
    case SSLEEP: "S"
    case SSTOP: "T"
    case SZOMB: "Z"
    default: "?"
    }
}

private func processCommand(_ process: kinfo_proc) -> String {
    withUnsafeBytes(of: process.kp_proc.p_comm) { bytes in
        let command = String(decoding: bytes.prefix { $0 != 0 }, as: UTF8.self)
        return command.isEmpty ? "?" : command
    }
}

private func readProcesses() throws -> [kinfo_proc] {
    for _ in 0..<3 {
        var mib = [CTL_KERN, KERN_PROC, KERN_PROC_ALL, 0]
        var byteCount = 0
        guard sysctl(&mib, u_int(mib.count), nil, &byteCount, nil, 0) == 0 else {
            throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
        }
        guard byteCount >= MemoryLayout<kinfo_proc>.size else { return [] }
        var processes = [kinfo_proc](
            repeating: kinfo_proc(),
            count: byteCount / MemoryLayout<kinfo_proc>.stride
        )
        let result = processes.withUnsafeMutableBufferPointer { buffer in
            sysctl(
                &mib,
                u_int(mib.count),
                buffer.baseAddress.map(UnsafeMutableRawPointer.init),
                &byteCount,
                nil,
                0
            )
        }
        if result == 0 {
            guard byteCount % MemoryLayout<kinfo_proc>.stride == 0 else {
                throw POSIXError(.EIO)
            }
            return Array(processes.prefix(byteCount / MemoryLayout<kinfo_proc>.stride))
        }
        if errno != ENOMEM {
            throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
        }
    }
    throw POSIXError(.ENOMEM)
}

private let requestedArguments = Array(CommandLine.arguments.dropFirst())
private let outputFormat: OutputFormat
switch requestedArguments {
case ["-axo", "pid=,ppid=,pgid=,uid=,stat=,command="]:
    outputFormat = .ownership
case ["-axo", "pid=,ppid=,pgid=,sess=,command="]:
    outputFormat = .sampling
default:
    FileHandle.standardError.write(Data(
        "chatos_plugin_ps only supports ChatOS process ownership and sampling queries\n".utf8
    ))
    exit(EX_USAGE)
}

do {
    let currentUID = getuid()
    let lines = try readProcesses()
        .filter { $0.kp_eproc.e_ucred.cr_uid == currentUID }
        .sorted { $0.kp_proc.p_pid < $1.kp_proc.p_pid }
        .map { process in
            let prefix = [
                String(process.kp_proc.p_pid),
                String(process.kp_eproc.e_ppid),
                String(process.kp_eproc.e_pgid),
            ]
            switch outputFormat {
            case .ownership:
                return (prefix + [
                    String(process.kp_eproc.e_ucred.cr_uid),
                    String(processState(process.kp_proc.p_stat)),
                    processCommand(process),
                ]).joined(separator: " ")
            case .sampling:
                let sessionID = getsid(process.kp_proc.p_pid)
                return (prefix + [
                    String(sessionID < 0 ? 0 : sessionID),
                    processCommand(process),
                ]).joined(separator: " ")
            }
        }
        .joined(separator: "\n")
    if !lines.isEmpty {
        FileHandle.standardOutput.write(Data((lines + "\n").utf8))
    }
} catch {
    FileHandle.standardError.write(Data("chatos_plugin_ps: \(error.localizedDescription)\n".utf8))
    exit(EX_OSERR)
}
