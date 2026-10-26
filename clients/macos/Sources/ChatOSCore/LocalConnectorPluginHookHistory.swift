import Foundation

public struct LocalConnectorPluginHookInvocation: Codable, Identifiable, Sendable, Equatable {
    public var invocationID: String
    public var bundleID: String
    public var hookID: String
    public var eventType: String
    public var isAsync: Bool
    public var attempt: UInt32
    public var status: String
    public var exitCode: Int32?
    public var timedOut: Bool
    public var durationMs: UInt64
    public var stdoutBytes: UInt64
    public var stdoutOverflow: Bool
    public var stderrBytes: UInt64
    public var stderrOverflow: Bool
    public var createdAtUnixMs: Int64
    public var updatedAtUnixMs: Int64

    public var id: String { invocationID }

    public init(
        invocationID: String,
        bundleID: String,
        hookID: String,
        eventType: String,
        isAsync: Bool,
        attempt: UInt32,
        status: String,
        exitCode: Int32?,
        timedOut: Bool,
        durationMs: UInt64,
        stdoutBytes: UInt64,
        stdoutOverflow: Bool,
        stderrBytes: UInt64,
        stderrOverflow: Bool,
        createdAtUnixMs: Int64,
        updatedAtUnixMs: Int64
    ) {
        self.invocationID = invocationID
        self.bundleID = bundleID
        self.hookID = hookID
        self.eventType = eventType
        self.isAsync = isAsync
        self.attempt = attempt
        self.status = status
        self.exitCode = exitCode
        self.timedOut = timedOut
        self.durationMs = durationMs
        self.stdoutBytes = stdoutBytes
        self.stdoutOverflow = stdoutOverflow
        self.stderrBytes = stderrBytes
        self.stderrOverflow = stderrOverflow
        self.createdAtUnixMs = createdAtUnixMs
        self.updatedAtUnixMs = updatedAtUnixMs
    }
}

public struct LocalConnectorPluginHookHistoryPage: Codable, Sendable, Equatable {
    public var invocations: [LocalConnectorPluginHookInvocation]
    public var nextBeforeUpdatedAtUnixMs: Int64?
    public var nextBeforeInvocationID: String?

    public init(
        invocations: [LocalConnectorPluginHookInvocation],
        nextBeforeUpdatedAtUnixMs: Int64?,
        nextBeforeInvocationID: String?
    ) {
        self.invocations = invocations
        self.nextBeforeUpdatedAtUnixMs = nextBeforeUpdatedAtUnixMs
        self.nextBeforeInvocationID = nextBeforeInvocationID
    }
}
