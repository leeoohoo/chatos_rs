import Foundation

public enum PluginApplicationContextSelection: Sendable, Equatable {
    case none
    case device
    case project(id: String)
}

public struct PluginApplicationBridgeContext: Sendable, Equatable {
    public let projectID: String?
    public let projectName: String?

    public init(projectID: String?, projectName: String?) {
        self.projectID = projectID
        self.projectName = projectName
    }
}

public struct PluginApplicationLaunchResult: Sendable, Equatable {
    public let launch: LocalConnectorPluginApplicationLaunch
    public let bridgeContext: PluginApplicationBridgeContext?

    public init(
        launch: LocalConnectorPluginApplicationLaunch,
        bridgeContext: PluginApplicationBridgeContext?
    ) {
        self.launch = launch
        self.bridgeContext = bridgeContext
    }
}

public enum PluginApplicationLaunchError: LocalizedError, Sendable, Equatable {
    case invalidOwner
    case contextRequired
    case invalidContextSelection
    case invalidProject

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "插件应用缺少有效的账号上下文。"
        case .contextRequired:
            "请先选择插件应用使用的项目。"
        case .invalidContextSelection:
            "插件应用不允许使用这个上下文。"
        case .invalidProject:
            "插件应用的项目标识无效。"
        }
    }
}

public struct PluginApplicationLaunchUseCase: Sendable {
    private let contextProvider: any PluginApplicationContextProviding
    private let launcher: any LocalConnectorPluginApplicationLaunching

    public init(
        contextProvider: any PluginApplicationContextProviding,
        launcher: any LocalConnectorPluginApplicationLaunching
    ) {
        self.contextProvider = contextProvider
        self.launcher = launcher
    }

    public func launch(
        application: LocalConnectorPluginApplication,
        ownerUserID ownerInput: String,
        context selection: PluginApplicationContextSelection
    ) async throws -> PluginApplicationLaunchResult {
        let ownerUserID = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !ownerUserID.isEmpty else { throw PluginApplicationLaunchError.invalidOwner }

        let requiresProjectContext = application.contextScope == "project"
            || application.contextScope == "workspace"
        let resolvedContext: LocalConnectorPluginApplicationContext?
        switch selection {
        case .none:
            guard !requiresProjectContext else {
                throw PluginApplicationLaunchError.contextRequired
            }
            resolvedContext = nil
        case .device:
            guard requiresProjectContext, application.missingContext == "device" else {
                throw PluginApplicationLaunchError.invalidContextSelection
            }
            resolvedContext = .device
        case let .project(id):
            guard requiresProjectContext else {
                throw PluginApplicationLaunchError.invalidContextSelection
            }
            let projectID = id.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !projectID.isEmpty else { throw PluginApplicationLaunchError.invalidProject }
            resolvedContext = try await contextProvider.pluginContext(
                ownerUserID: ownerUserID,
                projectID: projectID
            )
        }

        try Task.checkCancellation()
        let launch = try await launcher.launchPluginApplication(
            pluginID: application.pluginID,
            componentKey: application.componentKey,
            context: resolvedContext,
            expectedOwnerUserID: ownerUserID
        )
        try Task.checkCancellation()
        return PluginApplicationLaunchResult(
            launch: launch,
            bridgeContext: resolvedContext.map {
                PluginApplicationBridgeContext(
                    projectID: $0.projectID,
                    projectName: $0.projectName
                )
            }
        )
    }
}

public struct PluginApplicationLaunchToken: Sendable, Equatable {
    public let ownerUserID: String
    public let accountGeneration: UInt64
    public let applicationID: String
    public let selection: PluginApplicationContextSelection
    public let requestID: UUID

    public init(
        ownerUserID: String,
        accountGeneration: UInt64,
        applicationID: String,
        selection: PluginApplicationContextSelection,
        requestID: UUID
    ) {
        self.ownerUserID = ownerUserID
        self.accountGeneration = accountGeneration
        self.applicationID = applicationID
        self.selection = selection
        self.requestID = requestID
    }
}

public struct PluginApplicationLaunchState: Sendable, Equatable {
    public private(set) var activeToken: PluginApplicationLaunchToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountGeneration: UInt64,
        applicationID: String,
        selection: PluginApplicationContextSelection
    ) -> PluginApplicationLaunchToken {
        let token = PluginApplicationLaunchToken(
            ownerUserID: ownerUserID,
            accountGeneration: accountGeneration,
            applicationID: applicationID,
            selection: selection,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: PluginApplicationLaunchToken,
        ownerUserID: String,
        accountGeneration: UInt64,
        applicationID: String
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountGeneration == accountGeneration
            && token.applicationID == applicationID
    }

    @discardableResult
    public mutating func finish(_ token: PluginApplicationLaunchToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
