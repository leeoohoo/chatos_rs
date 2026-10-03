import Foundation

actor NativeLocalAgentTerminalStore {
    private let terminal: NativeMCPTerminalStore

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
              case .string? = values["terminal_id"] else {
            throw NativeLocalAgentPlatformToolError.projectToolFailed
        }
        return result
    }

    func call(
        name: String,
        arguments: [String: NativeJSONValue],
        projectRoot: URL,
        ownerRunID: String
    ) async throws -> NativeJSONValue {
        return try await terminal.call(
            name: name,
            arguments: arguments,
            projectRoot: projectRoot,
            ownerRunID: ownerRunID
        )
    }

    @discardableResult
    func cancel(ownerRunID: String) async -> Int {
        await terminal.cancel(ownerRunID: ownerRunID)
    }

    func cancelAll() async {
        _ = await terminal.cancelAllOwnedProcesses()
    }
}
