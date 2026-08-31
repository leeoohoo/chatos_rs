import ChatOSCore
import Foundation

@MainActor
final class TurnProcessViewModel: ObservableObject {
    @Published private(set) var nodes: [TurnProcessNode] = []
    @Published private(set) var isLoading = false
    @Published private(set) var errorMessage: String?

    let turn: ConversationTurn
    private let service: any TurnProcessServicing

    init(turn: ConversationTurn, service: any TurnProcessServicing) {
        self.turn = turn
        self.service = service
    }

    func load() {
        guard !isLoading else { return }
        isLoading = true
        errorMessage = nil

        Task {
            do {
                nodes = try await service.fetchProcessNodes(
                    sessionID: turn.sessionID,
                    turnID: turn.id
                )
            } catch {
                errorMessage = error.localizedDescription
            }
            isLoading = false
        }
    }
}
