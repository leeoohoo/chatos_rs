import Foundation

public struct ProjectRunTarget: Sendable, Equatable, Hashable, Identifiable {
    public var id: String
    public var label: String
    public var kind: String
    public var language: String?
    public var cwd: String
    public var command: String
    public var source: String
    public var isDefault: Bool
    public var entrypoint: String?
    public var manifestPath: String?
    public var requiredToolchains: [String]

    public init(
        id: String,
        label: String,
        kind: String,
        language: String?,
        cwd: String,
        command: String,
        source: String,
        isDefault: Bool,
        entrypoint: String?,
        manifestPath: String?,
        requiredToolchains: [String]
    ) {
        self.id = id
        self.label = label
        self.kind = kind
        self.language = language
        self.cwd = cwd
        self.command = command
        self.source = source
        self.isDefault = isDefault
        self.entrypoint = entrypoint
        self.manifestPath = manifestPath
        self.requiredToolchains = requiredToolchains
    }
}

public struct ProjectRunCatalog: Sendable, Equatable {
    public var projectID: String
    public var status: String
    public var defaultTargetID: String?
    public var targets: [ProjectRunTarget]
    public var errorMessage: String?

    public init(projectID: String, status: String, defaultTargetID: String?, targets: [ProjectRunTarget], errorMessage: String?) {
        self.projectID = projectID
        self.status = status
        self.defaultTargetID = defaultTargetID
        self.targets = targets
        self.errorMessage = errorMessage
    }
}

public struct ProjectRunInstance: Sendable, Equatable, Hashable, Identifiable {
    public var id: String
    public var name: String
    public var cwd: String?
    public var status: String
    public var isBusy: Bool
    public var isRunning: Bool
    public var log: String?
    public var startedAt: Date?
    public var exitCode: Int32?

    public init(
        id: String,
        name: String,
        cwd: String?,
        status: String,
        isBusy: Bool,
        isRunning: Bool,
        log: String? = nil,
        startedAt: Date? = nil,
        exitCode: Int32? = nil
    ) {
        self.id = id
        self.name = name
        self.cwd = cwd
        self.status = status
        self.isBusy = isBusy
        self.isRunning = isRunning
        self.log = log
        self.startedAt = startedAt
        self.exitCode = exitCode
    }
}

public struct ProjectRunState: Sendable, Equatable {
    public var projectID: String
    public var status: String
    public var isBusy: Bool
    public var isRunning: Bool
    public var instances: [ProjectRunInstance]

    public init(projectID: String, status: String, isBusy: Bool, isRunning: Bool, instances: [ProjectRunInstance]) {
        self.projectID = projectID
        self.status = status
        self.isBusy = isBusy
        self.isRunning = isRunning
        self.instances = instances
    }
}

public struct ProjectRunValidationIssue: Sendable, Equatable, Hashable, Identifiable {
    public var id: String { "\(kind):\(targetID ?? ""):\(path ?? message)" }
    public var kind: String
    public var message: String
    public var targetID: String?
    public var targetLabel: String?
    public var path: String?
    public var hint: String?

    public init(kind: String, message: String, targetID: String?, targetLabel: String?, path: String?, hint: String?) {
        self.kind = kind
        self.message = message
        self.targetID = targetID
        self.targetLabel = targetLabel
        self.path = path
        self.hint = hint
    }
}

public struct ProjectRunEnvironment: Sendable, Equatable {
    public var toolchainOptions: [String: [ProjectRunToolchainOption]]
    public var configurationFiles: [ProjectRunConfigurationFile]
    public var validationIssues: [ProjectRunValidationIssue]
    public var selectedToolchains: [String: String]
    public var customToolchains: [String: ProjectRunCustomToolchain]
    public var environmentVariables: [String: String]
    public var terminalUIEnabled: Bool

    public init(
        toolchainOptions: [String: [ProjectRunToolchainOption]] = [:],
        configurationFiles: [ProjectRunConfigurationFile] = [],
        validationIssues: [ProjectRunValidationIssue],
        selectedToolchains: [String: String],
        customToolchains: [String: ProjectRunCustomToolchain] = [:],
        environmentVariables: [String: String],
        terminalUIEnabled: Bool
    ) {
        self.toolchainOptions = toolchainOptions
        self.configurationFiles = configurationFiles
        self.validationIssues = validationIssues
        self.selectedToolchains = selectedToolchains
        self.customToolchains = customToolchains
        self.environmentVariables = environmentVariables
        self.terminalUIEnabled = terminalUIEnabled
    }
}

public struct ProjectRunToolchainOption: Sendable, Equatable, Hashable, Identifiable {
    public var id: String
    public var kind: String
    public var label: String
    public var version: String?
    public var path: String
    public var source: String
    public var isDefault: Bool

    public init(id: String, kind: String, label: String, version: String?, path: String, source: String, isDefault: Bool) {
        self.id = id
        self.kind = kind
        self.label = label
        self.version = version
        self.path = path
        self.source = source
        self.isDefault = isDefault
    }
}

public struct ProjectRunConfigurationFile: Sendable, Equatable, Hashable, Identifiable {
    public var id: String { "\(kind):\(path)" }
    public var kind: String
    public var label: String
    public var path: String
    public var preview: String?
    public var source: String

    public init(kind: String, label: String, path: String, preview: String?, source: String) {
        self.kind = kind
        self.label = label
        self.path = path
        self.preview = preview
        self.source = source
    }
}

public struct ProjectRunCustomToolchain: Codable, Sendable, Equatable, Hashable {
    public var kind: String
    public var label: String
    public var path: String

    public init(kind: String, label: String, path: String) {
        self.kind = kind
        self.label = label
        self.path = path
    }
}

public protocol ProjectRunServicing: Sendable {
    func fetchCatalog(projectID: String) async throws -> ProjectRunCatalog
    func analyze(projectID: String) async throws -> ProjectRunCatalog
    func fetchState(projectID: String) async throws -> ProjectRunState
    func fetchEnvironment(projectID: String) async throws -> ProjectRunEnvironment
    func updateEnvironment(
        projectID: String,
        selectedToolchains: [String: String],
        customToolchains: [String: ProjectRunCustomToolchain],
        environmentVariables: [String: String]
    ) async throws -> ProjectRunEnvironment
    func setDefaultTarget(projectID: String, targetID: String) async throws -> ProjectRunCatalog
    func start(projectID: String, targetID: String) async throws
    func stop(instanceID: String) async throws
    func delete(instanceID: String) async throws
}
