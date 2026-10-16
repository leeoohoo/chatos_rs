import Foundation

public struct LocalConnectorSandboxBackend: Codable, Identifiable, Sendable, Equatable {
    public var backend: String
    public var status: String
    public var selectable: Bool
    public var filesystemIsolation: Bool
    public var networkIsolation: Bool
    public var processTreeControl: Bool
    public var message: String

    public var id: String { backend }

    public init(
        backend: String,
        status: String,
        selectable: Bool,
        filesystemIsolation: Bool,
        networkIsolation: Bool,
        processTreeControl: Bool,
        message: String
    ) {
        self.backend = backend
        self.status = status
        self.selectable = selectable
        self.filesystemIsolation = filesystemIsolation
        self.networkIsolation = networkIsolation
        self.processTreeControl = processTreeControl
        self.message = message
    }
}

public struct LocalConnectorSandboxSettings: Codable, Sendable, Equatable {
    public var enabled: Bool
    public var defaultBackend: String
    public var defaultPermissionProfileID: String
    public var defaultPermissionProfileName: String
    public var defaultApprovalPolicy: String
    public var defaultApprovalReviewer: String
    public var defaultNetworkAccess: String
    public var permissionConfigurationError: String?
    public var policyRevision: String?

    public init(
        enabled: Bool,
        defaultBackend: String,
        defaultPermissionProfileID: String,
        defaultPermissionProfileName: String,
        defaultApprovalPolicy: String,
        defaultApprovalReviewer: String,
        defaultNetworkAccess: String,
        permissionConfigurationError: String?,
        policyRevision: String?
    ) {
        self.enabled = enabled
        self.defaultBackend = defaultBackend
        self.defaultPermissionProfileID = defaultPermissionProfileID
        self.defaultPermissionProfileName = defaultPermissionProfileName
        self.defaultApprovalPolicy = defaultApprovalPolicy
        self.defaultApprovalReviewer = defaultApprovalReviewer
        self.defaultNetworkAccess = defaultNetworkAccess
        self.permissionConfigurationError = permissionConfigurationError
        self.policyRevision = policyRevision
    }
}

public enum LocalConnectorPluginInstallationIssue: String, Codable, Sendable, Equatable {
    case missingIntegrityRecord = "missing_integrity_record"
    case integrityCheckFailed = "integrity_check_failed"
}

public enum LocalConnectorPluginSource: String, Codable, Sendable, Equatable {
    case marketplace
    case customMCP = "custom_mcp"
    case localPlugin = "local_plugin"
    case gitPlugin = "git_plugin"
    case npmPlugin = "npm_plugin"
}

public struct LocalConnectorPluginSourceDraft: Codable, Sendable, Equatable {
    public enum Kind: String, Codable, Sendable, Equatable {
        case localFolder = "local_folder"
        case gitRepository = "git_repository"
        case npmPackage = "npm_package"
    }

    public var kind: Kind
    public var location: String
    public var exactVersion: String?
    public var gitReference: String?
    public var subdirectory: String?

    public init(
        kind: Kind,
        location: String,
        exactVersion: String? = nil,
        gitReference: String? = nil,
        subdirectory: String? = nil
    ) {
        self.kind = kind
        self.location = location
        self.exactVersion = exactVersion
        self.gitReference = gitReference
        self.subdirectory = subdirectory
    }
}

public struct LocalConnectorCustomMCPDraft: Codable, Sendable, Equatable {
    public var displayName: String
    public var description: String
    public var componentKey: String
    public var packageName: String
    public var packageVersion: String
    public var binaryName: String
    public var arguments: [String]
    public var environment: [String: String]

    public init(
        displayName: String,
        description: String = "",
        componentKey: String,
        packageName: String,
        packageVersion: String,
        binaryName: String,
        arguments: [String] = [],
        environment: [String: String] = [:]
    ) {
        self.displayName = displayName
        self.description = description
        self.componentKey = componentKey
        self.packageName = packageName
        self.packageVersion = packageVersion
        self.binaryName = binaryName
        self.arguments = arguments
        self.environment = environment
    }
}

public struct LocalConnectorPluginInspectionComponent: Codable, Identifiable, Sendable, Equatable {
    public enum Kind: String, Codable, Sendable, Equatable {
        case mcp
        case skill
    }

    public var componentKey: String
    public var displayName: String
    public var kind: Kind
    public var toolNames: [String]

    public var id: String { componentKey }

    public init(
        componentKey: String,
        displayName: String,
        kind: Kind,
        toolNames: [String]
    ) {
        self.componentKey = componentKey
        self.displayName = displayName
        self.kind = kind
        self.toolNames = toolNames
    }
}

public struct LocalConnectorPluginInspection: Codable, Sendable, Equatable {
    public var fingerprint: String
    public var displayName: String
    public var version: String
    public var sourceSummary: String
    public var components: [LocalConnectorPluginInspectionComponent]

    public init(
        fingerprint: String,
        displayName: String,
        version: String,
        sourceSummary: String,
        components: [LocalConnectorPluginInspectionComponent]
    ) {
        self.fingerprint = fingerprint
        self.displayName = displayName
        self.version = version
        self.sourceSummary = sourceSummary
        self.components = components
    }
}

public struct LocalConnectorPlugin: Codable, Identifiable, Sendable, Equatable {
    public var pluginID: String
    public var packageName: String?
    public var pluginKey: String?
    public var displayName: String
    public var description: String
    public var category: String
    public var publisher: String
    public var latestVersion: String
    public var installedVersion: String?
    public var installed: Bool
    public var updateAvailable: Bool
    public var installAvailable: Bool
    public var enabled: Bool
    public var hasUI: Bool?
    public var permissions: [LocalConnectorPluginPermission]
    public var installationIssue: LocalConnectorPluginInstallationIssue?
    public var source: LocalConnectorPluginSource

    public var id: String { pluginID }
    public var requiresReinstall: Bool { installationIssue != nil }

    public init(
        pluginID: String,
        packageName: String? = nil,
        pluginKey: String? = nil,
        displayName: String,
        description: String,
        category: String,
        publisher: String,
        latestVersion: String,
        installedVersion: String? = nil,
        installed: Bool,
        updateAvailable: Bool,
        installAvailable: Bool,
        enabled: Bool,
        hasUI: Bool? = nil,
        permissions: [LocalConnectorPluginPermission] = [],
        installationIssue: LocalConnectorPluginInstallationIssue? = nil,
        source: LocalConnectorPluginSource = .marketplace
    ) {
        self.pluginID = pluginID
        self.packageName = packageName
        self.pluginKey = pluginKey
        self.displayName = displayName
        self.description = description
        self.category = category
        self.publisher = publisher
        self.latestVersion = latestVersion
        self.installedVersion = installedVersion
        self.installed = installed
        self.updateAvailable = updateAvailable
        self.installAvailable = installAvailable
        self.enabled = enabled
        self.hasUI = hasUI
        self.permissions = permissions
        self.installationIssue = installationIssue
        self.source = source
    }
}

public struct LocalConnectorPluginApplication: Codable, Identifiable, Sendable, Equatable {
    public var pluginID: String
    public var componentKey: String
    public var displayName: String
    public var description: String
    public var brandColor: String?
    public var iconURL: URL?
    public var requiresLocalRuntime: Bool
    public var contextScope: String?
    public var missingContext: String?
    public var bridgeCapabilities: [String]

    public var id: String { "\(pluginID):\(componentKey)" }

    public init(
        pluginID: String,
        componentKey: String,
        displayName: String,
        description: String,
        brandColor: String? = nil,
        iconURL: URL? = nil,
        requiresLocalRuntime: Bool,
        contextScope: String? = nil,
        missingContext: String? = nil,
        bridgeCapabilities: [String] = []
    ) {
        self.pluginID = pluginID
        self.componentKey = componentKey
        self.displayName = displayName
        self.description = description
        self.brandColor = brandColor
        self.iconURL = iconURL
        self.requiresLocalRuntime = requiresLocalRuntime
        self.contextScope = contextScope
        self.missingContext = missingContext
        self.bridgeCapabilities = bridgeCapabilities
    }
}

public struct LocalConnectorPluginApplicationContext: Codable, Sendable, Equatable, Hashable {
    public var projectID: String?
    public var projectName: String?
    public var projectRoot: String?

    public init(projectID: String?, projectName: String?, projectRoot: String?) {
        self.projectID = projectID
        self.projectName = projectName
        self.projectRoot = projectRoot
    }

    public static let device = Self(projectID: nil, projectName: nil, projectRoot: nil)
}

public struct LocalConnectorPluginApplicationLaunch: Codable, Sendable, Equatable {
    public var application: LocalConnectorPluginApplication
    public var url: URL
    public var websiteDataStoreID: UUID?
    public var releaseID: String
    public var version: String
    public var artifactSHA256: String

    public init(
        application: LocalConnectorPluginApplication,
        url: URL,
        websiteDataStoreID: UUID? = nil,
        releaseID: String = "",
        version: String = "",
        artifactSHA256: String = ""
    ) {
        self.application = application
        self.url = url
        self.websiteDataStoreID = websiteDataStoreID
        self.releaseID = releaseID
        self.version = version
        self.artifactSHA256 = artifactSHA256
    }
}

public struct LocalConnectorPluginPermission: Codable, Identifiable, Sendable, Equatable {
    public var permissionID: String
    public var label: String
    public var summary: String
    public var required: Bool
    public var status: String
    public var statusLabel: String
    public var canRequest: Bool
    public var requestLabel: String
    public var settingsTarget: String?
    public var note: String?

    public var id: String { permissionID }

    public init(
        permissionID: String,
        label: String,
        summary: String,
        required: Bool,
        status: String,
        statusLabel: String,
        canRequest: Bool,
        requestLabel: String,
        settingsTarget: String? = nil,
        note: String? = nil
    ) {
        self.permissionID = permissionID
        self.label = label
        self.summary = summary
        self.required = required
        self.status = status
        self.statusLabel = statusLabel
        self.canRequest = canRequest
        self.requestLabel = requestLabel
        self.settingsTarget = settingsTarget
        self.note = note
    }
}
