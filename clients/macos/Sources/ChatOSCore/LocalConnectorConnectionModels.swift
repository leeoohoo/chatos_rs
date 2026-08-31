import Foundation

public struct LocalConnectorUser: Codable, Sendable, Equatable {
    public var id: String
    public var username: String
    public var displayName: String?
    public var role: String

    public init(id: String, username: String, displayName: String?, role: String) {
        self.id = id
        self.username = username
        self.displayName = displayName
        self.role = role
    }
}

public struct LocalConnectorWorkspace: Codable, Identifiable, Sendable, Equatable {
    public var id: String
    public var alias: String
    public var absoluteRoot: String
    public var fingerprint: String
    public var projectConfigTrusted: Bool?
    public var projectConfigTrustStale: Bool?

    public init(
        id: String,
        alias: String,
        absoluteRoot: String,
        fingerprint: String,
        projectConfigTrusted: Bool? = nil,
        projectConfigTrustStale: Bool? = nil
    ) {
        self.id = id
        self.alias = alias
        self.absoluteRoot = absoluteRoot
        self.fingerprint = fingerprint
        self.projectConfigTrusted = projectConfigTrusted
        self.projectConfigTrustStale = projectConfigTrustStale
    }
}

public struct LocalConnectorStatus: Codable, Sendable, Equatable {
    public var configured: Bool
    public var connectorRunning: Bool
    public var developerMode: Bool
    public var cloudBaseURL: String?
    public var userServiceBaseURL: String?
    public var deviceID: String?
    public var deviceName: String?
    public var user: LocalConnectorUser?
    public var defaultWorkspaceID: String?
    public var workspaces: [LocalConnectorWorkspace]

    public init(
        configured: Bool,
        connectorRunning: Bool,
        developerMode: Bool,
        cloudBaseURL: String?,
        userServiceBaseURL: String?,
        deviceID: String?,
        deviceName: String?,
        user: LocalConnectorUser?,
        defaultWorkspaceID: String?,
        workspaces: [LocalConnectorWorkspace]
    ) {
        self.configured = configured
        self.connectorRunning = connectorRunning
        self.developerMode = developerMode
        self.cloudBaseURL = cloudBaseURL
        self.userServiceBaseURL = userServiceBaseURL
        self.deviceID = deviceID
        self.deviceName = deviceName
        self.user = user
        self.defaultWorkspaceID = defaultWorkspaceID
        self.workspaces = workspaces
    }
}

public struct LocalConnectorRuntimeSettings: Codable, Sendable, Equatable {
    public var developerMode: Bool
    public var developerCloudBaseURL: String
    public var developerUserServiceBaseURL: String
    public var developerChatOSWebURL: String

    public init(
        developerMode: Bool,
        developerCloudBaseURL: String,
        developerUserServiceBaseURL: String,
        developerChatOSWebURL: String
    ) {
        self.developerMode = developerMode
        self.developerCloudBaseURL = developerCloudBaseURL
        self.developerUserServiceBaseURL = developerUserServiceBaseURL
        self.developerChatOSWebURL = developerChatOSWebURL
    }
}

public struct LocalConnectorSystemPermission: Codable, Identifiable, Sendable, Equatable {
    public var id: String
    public var label: String
    public var summary: String
    public var status: String
    public var statusLabel: String
    public var required: Bool
    public var canRequest: Bool
    public var requestLabel: String
    public var note: String
    public var lastError: String?

    public init(
        id: String,
        label: String,
        summary: String,
        status: String,
        statusLabel: String,
        required: Bool,
        canRequest: Bool,
        requestLabel: String,
        note: String,
        lastError: String?
    ) {
        self.id = id
        self.label = label
        self.summary = summary
        self.status = status
        self.statusLabel = statusLabel
        self.required = required
        self.canRequest = canRequest
        self.requestLabel = requestLabel
        self.note = note
        self.lastError = lastError
    }
}

public struct LocalConnectorSystemPermissions: Codable, Sendable, Equatable {
    public var platform: String
    public var platformLabel: String
    public var items: [LocalConnectorSystemPermission]

    public init(platform: String, platformLabel: String, items: [LocalConnectorSystemPermission]) {
        self.platform = platform
        self.platformLabel = platformLabel
        self.items = items
    }
}

public struct LocalConnectorTerminalResult: Codable, Sendable, Equatable {
    public var command: String
    public var args: [String]
    public var cwd: String
    public var success: Bool
    public var exitCode: Int?
    public var timedOut: Bool
    public var stdout: String
    public var stderr: String
    public var error: String?

    public init(
        command: String,
        args: [String],
        cwd: String,
        success: Bool,
        exitCode: Int?,
        timedOut: Bool,
        stdout: String,
        stderr: String,
        error: String?
    ) {
        self.command = command
        self.args = args
        self.cwd = cwd
        self.success = success
        self.exitCode = exitCode
        self.timedOut = timedOut
        self.stdout = stdout
        self.stderr = stderr
        self.error = error
    }
}

public struct LocalConnectorCommandHistoryEntry: Codable, Identifiable, Sendable, Equatable {
    public var id: String
    public var source: String
    public var workspaceAlias: String?
    public var cwd: String?
    public var display: String
    public var status: String
    public var exitCode: Int?
    public var stdoutPreview: String?
    public var stderrPreview: String?
    public var error: String?
    public var startedAt: String

    public init(
        id: String,
        source: String,
        workspaceAlias: String?,
        cwd: String?,
        display: String,
        status: String,
        exitCode: Int?,
        stdoutPreview: String?,
        stderrPreview: String?,
        error: String?,
        startedAt: String
    ) {
        self.id = id
        self.source = source
        self.workspaceAlias = workspaceAlias
        self.cwd = cwd
        self.display = display
        self.status = status
        self.exitCode = exitCode
        self.stdoutPreview = stdoutPreview
        self.stderrPreview = stderrPreview
        self.error = error
        self.startedAt = startedAt
    }
}
