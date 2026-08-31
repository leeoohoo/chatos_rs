import Foundation
import SwiftUI

@MainActor
final class TerminalWorkspaceViewModel: ObservableObject {
    struct Session: Identifiable {
        let id: UUID
        let title: String
        let terminal: TerminalViewModel
    }

    @Published private(set) var sessions: [Session] = []
    @Published var selectedSessionID: UUID?

    private let workingDirectory: String
    private var nextTerminalNumber = 1

    init(workingDirectory: String = FileManager.default.currentDirectoryPath) {
        self.workingDirectory = workingDirectory
        createTerminal()
    }

    var selectedSession: Session? {
        guard let selectedSessionID else { return sessions.first }
        return sessions.first { $0.id == selectedSessionID }
    }

    func createTerminal() {
        let number = nextTerminalNumber
        nextTerminalNumber += 1

        let session = Session(
            id: UUID(),
            title: sessionTitle(number: number),
            terminal: TerminalViewModel(workingDirectory: workingDirectory)
        )
        sessions.append(session)
        selectedSessionID = session.id
    }

    func selectTerminal(id: UUID) {
        guard sessions.contains(where: { $0.id == id }) else { return }
        selectedSessionID = id
    }

    func closeTerminal(id: UUID) {
        guard let index = sessions.firstIndex(where: { $0.id == id }) else { return }
        let wasSelected = selectedSessionID == id
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

    private func sessionTitle(number: Int) -> String {
        let directoryName = URL(fileURLWithPath: workingDirectory).lastPathComponent
        let baseTitle = directoryName.isEmpty ? "终端" : directoryName
        return number == 1 ? baseTitle : "\(baseTitle) \(number)"
    }
}
