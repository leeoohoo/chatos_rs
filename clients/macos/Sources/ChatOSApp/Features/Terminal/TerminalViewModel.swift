import Foundation
import SwiftUI

@MainActor
final class TerminalViewModel: ObservableObject {
    @Published var command = ""
    @Published var searchText = ""
    @Published private(set) var isRunning = false
    @Published private(set) var lines: [TerminalOutputLine] = []
    @Published private(set) var focusRequestRevision = 0

    @Published private(set) var workingDirectory: String
    private let executor: any TerminalCommandExecuting

    init(
        workingDirectory: String = FileManager.default.currentDirectoryPath,
        executor: any TerminalCommandExecuting = ShellCommandExecutor()
    ) {
        self.workingDirectory = workingDirectory
        self.executor = executor
    }

    var visibleLines: [TerminalOutputLine] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return lines }
        return lines.filter { $0.text.localizedCaseInsensitiveContains(query) }
    }

    func submit() {
        let value = command.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty, !isRunning else { return }

        command = ""
        isRunning = true
        lines.append(.init(text: value, kind: .command))

        Task {
            let result = await executor.execute(
                command: value,
                workingDirectory: workingDirectory
            )
            apply(result)
        }
    }

    func clear() {
        lines.removeAll()
    }

    func appendSystemLine(_ text: String) {
        lines.append(.init(text: text, kind: .system))
    }

    private func apply(_ result: TerminalCommandResult) {
        if !result.output.isEmpty {
            lines.append(.init(text: result.output, kind: result.exitCode == 0 ? .output : .error))
        }
        if !result.error.isEmpty {
            lines.append(.init(text: result.error, kind: .error))
        }
        if let workingDirectory = result.workingDirectory,
           !workingDirectory.isEmpty {
            self.workingDirectory = workingDirectory
        }
        isRunning = false
        focusRequestRevision += 1
    }
}
