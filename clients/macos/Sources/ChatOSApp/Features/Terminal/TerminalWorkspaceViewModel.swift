import Foundation
import SwiftUI

@MainActor
final class TerminalWorkspaceViewModel: ObservableObject {
    @MainActor
    struct Session: Identifiable {
        let id: UUID
        let terminal: NativeLocalTerminalViewModel

        var title: String { terminal.title }
    }

    @Published private(set) var sessions: [Session] = []
    @Published var selectedSessionID: UUID?

    private let workingDirectory: String

    init(workingDirectory: String = FileManager.default.currentDirectoryPath) {
        self.workingDirectory = workingDirectory
        createTerminal()
    }

    var selectedSession: Session? {
        guard let selectedSessionID else { return sessions.first }
        return sessions.first { $0.id == selectedSessionID }
    }

    func createTerminal() {
        let session = Session(
            id: UUID(),
            terminal: NativeLocalTerminalViewModel(workingDirectory: workingDirectory)
        )
        sessions.append(session)
        selectedSessionID = session.id
    }

    func ensureTerminal() {
        if sessions.isEmpty { createTerminal() }
    }

    func selectTerminal(id: UUID) {
        guard sessions.contains(where: { $0.id == id }) else { return }
        selectedSessionID = id
    }

    func closeTerminal(id: UUID) {
        guard let index = sessions.firstIndex(where: { $0.id == id }) else { return }
        let wasSelected = selectedSessionID == id
        sessions[index].terminal.close()
        sessions.remove(at: index)

        if sessions.isEmpty {
            createTerminal()
        } else if wasSelected {
            selectedSessionID = sessions[min(index, sessions.count - 1)].id
        }
    }

    func closeSelectedTerminal() {
        guard let selectedSessionID else { return }
        closeTerminal(id: selectedSessionID)
    }

    func closeAllTerminals() {
        sessions.forEach { $0.terminal.close() }
        sessions.removeAll()
        selectedSessionID = nil
    }

}
