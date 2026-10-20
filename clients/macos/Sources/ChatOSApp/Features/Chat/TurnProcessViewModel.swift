import ChatOSCore
import Foundation

@MainActor
final class TurnProcessViewModel: ObservableObject {
    @Published private(set) var nodes: [TurnProcessNode] = []
    @Published private(set) var isLoading = false
    @Published private(set) var errorMessage: String?

    let turn: ConversationTurn
    private let useCase: TurnProcessUseCase
    private var lifecycleState = TurnProcessLifecycleState()
    private var loadTask: Task<Void, Never>?

    init(turn: ConversationTurn, service: any TurnProcessServicing) {
        self.turn = turn
        self.useCase = TurnProcessUseCase(service: service)
    }

    func load() {
        guard !isLoading else { return }
        loadTask?.cancel()
        let token = lifecycleState.begin(
            sessionID: turn.sessionID,
            turnID: turn.id
        )
        let useCase = useCase
        isLoading = true
        errorMessage = nil

        loadTask = Task { [weak self] in
            let result: Result<[TurnProcessNode], Error>
            do {
                result = .success(try await useCase.load(
                    sessionID: token.sessionID,
                    turnID: token.turnID
                ))
            } catch {
                result = .failure(error)
            }

            guard !Task.isCancelled,
                  let self,
                  self.lifecycleState.finish(token) else {
                return
            }

            switch result {
            case let .success(nodes):
                self.nodes = nodes
            case let .failure(error):
                self.errorMessage = error.localizedDescription
            }
            self.isLoading = false
            self.loadTask = nil
        }
    }

    func stop() {
        loadTask?.cancel()
        loadTask = nil
        lifecycleState.reset()
        isLoading = false
    }

    deinit {
        loadTask?.cancel()
    }
}
