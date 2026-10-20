import ChatOSCore
import Foundation

@MainActor
final class PluginApplicationHostViewModel: ObservableObject {
    enum Phase: Sendable, Equatable {
        case choosingContext
        case launching
        case ready
        case failed
        case inactive
    }

    @Published private(set) var phase: Phase
    @Published private(set) var launch: LocalConnectorPluginApplicationLaunch?
    @Published private(set) var bridgeContext: PluginApplicationBridgeContext?
    @Published private(set) var errorMessage: String?
    @Published private(set) var selectedProjectID: String?

    let application: LocalConnectorPluginApplication

    private let ownerUserID: String
    private let accountGeneration: UInt64
    private let useCase: PluginApplicationLaunchUseCase
    private var launchState = PluginApplicationLaunchState()
    private var launchTask: Task<Void, Never>?
    private var selectedContext: PluginApplicationContextSelection?
    private var hasActivated = false

    init(
        application: LocalConnectorPluginApplication,
        ownerUserID: String,
        accountGeneration: UInt64,
        useCase: PluginApplicationLaunchUseCase
    ) {
        self.application = application
        self.ownerUserID = ownerUserID
        self.accountGeneration = accountGeneration
        self.useCase = useCase
        self.phase = Self.requiresContext(application) ? .choosingContext : .inactive
    }

    var requiresContextSelection: Bool {
        Self.requiresContext(application)
    }

    var allowsDeviceContext: Bool {
        requiresContextSelection && application.missingContext == "device"
    }

    func activate() {
        guard !hasActivated else { return }
        hasActivated = true
        if requiresContextSelection {
            phase = .choosingContext
        } else {
            start(context: .none)
        }
    }

    func selectDeviceContext() {
        guard allowsDeviceContext else { return }
        selectedProjectID = nil
        start(context: .device)
    }

    func selectProject(id: String) {
        guard requiresContextSelection else { return }
        selectedProjectID = id
        start(context: .project(id: id))
    }

    func retry() {
        guard let selectedContext else { return }
        start(context: selectedContext)
    }

    func chooseAnotherContext() {
        guard requiresContextSelection else { return }
        launchTask?.cancel()
        launchTask = nil
        launchState.reset()
        launch = nil
        bridgeContext = nil
        errorMessage = nil
        selectedProjectID = nil
        selectedContext = nil
        phase = .choosingContext
    }

    func deactivate() {
        hasActivated = false
        launchTask?.cancel()
        launchTask = nil
        launchState.reset()
        phase = .inactive
    }

    private func start(context: PluginApplicationContextSelection) {
        launchTask?.cancel()
        selectedContext = context
        launch = nil
        bridgeContext = nil
        errorMessage = nil
        let token = launchState.begin(
            ownerUserID: ownerUserID,
            accountGeneration: accountGeneration,
            applicationID: application.id,
            selection: context
        )
        phase = .launching

        let useCase = useCase
        let application = application
        let ownerUserID = ownerUserID
        let accountGeneration = accountGeneration
        launchTask = Task { [weak self] in
            let result: Result<PluginApplicationLaunchResult, Error>
            do {
                result = .success(try await useCase.launch(
                    application: application,
                    ownerUserID: ownerUserID,
                    context: context
                ))
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled,
                  let self,
                  self.launchState.accepts(
                    token,
                    ownerUserID: ownerUserID,
                    accountGeneration: accountGeneration,
                    applicationID: application.id
                  ) else { return }
            switch result {
            case let .success(value):
                self.launch = value.launch
                self.bridgeContext = value.bridgeContext
                self.phase = .ready
            case let .failure(error):
                self.errorMessage = error.localizedDescription
                self.phase = .failed
            }
            guard self.launchState.finish(token) else { return }
            self.launchTask = nil
        }
    }

    private static func requiresContext(
        _ application: LocalConnectorPluginApplication
    ) -> Bool {
        application.contextScope == "project"
            || application.contextScope == "workspace"
    }
}
