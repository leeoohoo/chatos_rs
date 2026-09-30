import Foundation

actor NativeLocalAgentTerminalStore {
    private let terminal: NativeMCPTerminalStore
    private var ownerRunIDByProcessID: [String: String] = [:]

    init(terminal: NativeMCPTerminalStore = .init()) {
        self.terminal = terminal
    }

    func execute(
        command: String,
        cwd: URL,
        projectRoot: URL,
        background: Bool,
        timeoutMilliseconds: Int? = nil,
        ownerRunID: String
    ) async throws -> NativeJSONValue {
        let result = try await terminal.execute(
            command: command,
            cwd: cwd,
            projectRoot: projectRoot,
            background: background,
            timeoutMilliseconds: timeoutMilliseconds,
            ownerRunID: ownerRunID
        )
        guard case let .object(values) = result,
              case let .string(processID)? = values["terminal_id"] else {
            throw NativeLocalAgentPlatformToolError.projectToolFailed
        }
        ownerRunIDByProcessID[processID] = ownerRunID
        return result
    }

    func call(
        name: String,
        arguments: [String: NativeJSONValue],
        projectRoot: URL,
        ownerRunID: String
    ) async throws -> NativeJSONValue {
        guard case let .string(processID)? = arguments["terminal_id"],
              ownerRunIDByProcessID[processID] == ownerRunID else {
            throw NativeLocalAgentPlatformToolError.projectToolFailed
        }
        return try await terminal.call(
            name: name,
            arguments: arguments,
            projectRoot: projectRoot
        )
    }

    @discardableResult
    func cancel(ownerRunID: String) async -> Int {
        let count = await terminal.cancel(ownerRunID: ownerRunID)
        ownerRunIDByProcessID = ownerRunIDByProcessID.filter { $0.value != ownerRunID }
        return count
    }

    func cancelAll() async {
        for ownerRunID in Set(ownerRunIDByProcessID.values) {
            _ = await terminal.cancel(ownerRunID: ownerRunID)
        }
        ownerRunIDByProcessID.removeAll()
    }
}
