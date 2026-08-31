import Foundation

public struct ProjectExecutionConfirmationState: Sendable, Equatable {
    public enum Phase: Sendable, Equatable {
        case unavailable
        case planning
        case awaitingConfirmation
        case running
        case completed
        case blocked
        case failed
        case stopped
        case graphUnavailable
    }

    public var isProjectExecution: Bool
    public var graphReadyForConfirmation: Bool
    public var hasStartedTasks: Bool
    public var overallStatus: String
    public var identity: ProjectExecutionIdentity?
    public var phase: Phase

    public var canConfirm: Bool {
        phase == .awaitingConfirmation && graphReadyForConfirmation && identity != nil
    }

    public init(
        context: ProjectExecutionContext?,
        graph: MessageTaskGraphSnapshot?,
        conversationID: String
    ) {
        let tasks = graph?.nodes.map(\.task) ?? []
        let statuses = tasks.map { task in
            Self.normalized(task.status) ?? Self.normalized(task.lastRunStatus) ?? ""
        }
        let hasStartedTasks = tasks.contains { Self.nonEmpty($0.lastRunID) != nil }
        let ready = !tasks.isEmpty && tasks.allSatisfy { task in
            Self.nonEmpty(task.lastRunID) == nil
                && ["ready", "todo", "queued", "pending", "doing"]
                    .contains(Self.normalized(task.status) ?? "")
        }
        let metadataStatus = Self.normalized(context?.overallStatus)
            ?? Self.normalized(context?.confirmationStatus)
            ?? ""
        let terminalPlanStatuses = [
            "completed", "failed", "error", "blocked", "stopped", "cancelled", "canceled",
        ]
        let runningPlanStatuses = [
            "confirmed", "processing", "running", "executing", "in_progress",
        ]
        let planningPlanStatuses = ["planning", "planning_started"]
        let explicitlyAwaiting = metadataStatus == "awaiting_confirmation"
        let awaiting = !hasStartedTasks
            && !runningPlanStatuses.contains(metadataStatus)
            && !planningPlanStatuses.contains(metadataStatus)
            && (explicitlyAwaiting || (ready && !terminalPlanStatuses.contains(metadataStatus)))
        let startedStatus = Self.startedStatus(statuses)
        let overallStatus: String
        if awaiting {
            overallStatus = "awaiting_confirmation"
        } else if terminalPlanStatuses.contains(metadataStatus) {
            overallStatus = metadataStatus
        } else if hasStartedTasks, let startedStatus {
            // Once graph nodes have started, their current task status is more
            // authoritative than lagging execution-group metadata.
            overallStatus = startedStatus
        } else if runningPlanStatuses.contains(metadataStatus)
                    || planningPlanStatuses.contains(metadataStatus) {
            overallStatus = metadataStatus
        } else {
            overallStatus = startedStatus ?? metadataStatus
        }

        self.isProjectExecution = context?.isProjectExecution == true
        self.graphReadyForConfirmation = ready
        self.hasStartedTasks = hasStartedTasks
        self.overallStatus = overallStatus
        self.identity = Self.identity(context: context, conversationID: conversationID)
        self.phase = Self.phase(
            isProjectExecution: isProjectExecution,
            awaiting: awaiting,
            graphReady: ready,
            hasStartedTasks: hasStartedTasks,
            graphHasTasks: !tasks.isEmpty,
            status: overallStatus
        )
    }

    private static func identity(
        context: ProjectExecutionContext?,
        conversationID: String
    ) -> ProjectExecutionIdentity? {
        guard let projectID = nonEmpty(context?.projectID),
              let requirementID = nonEmpty(context?.requirementID),
              let executionGroupID = nonEmpty(context?.executionGroupID),
              let conversationID = nonEmpty(conversationID) else { return nil }
        return ProjectExecutionIdentity(
            projectID: projectID,
            requirementID: requirementID,
            executionGroupID: executionGroupID,
            conversationID: conversationID,
            contactID: nonEmpty(context?.contactID)
        )
    }

    private static func startedStatus(_ statuses: [String]) -> String? {
        guard !statuses.isEmpty else { return nil }
        if statuses.contains("blocked") { return "blocked" }
        if statuses.contains(where: { ["failed", "error"].contains($0) }) { return "failed" }
        if statuses.contains(where: { ["cancelled", "canceled"].contains($0) }) { return "cancelled" }
        if statuses.allSatisfy({ ["completed", "succeeded", "success", "archived"].contains($0) }) {
            return "completed"
        }
        return "processing"
    }

    private static func phase(
        isProjectExecution: Bool,
        awaiting: Bool,
        graphReady: Bool,
        hasStartedTasks: Bool,
        graphHasTasks: Bool,
        status: String
    ) -> Phase {
        guard isProjectExecution else { return .unavailable }
        if !graphHasTasks && status == "completed" { return .graphUnavailable }
        if status == "stopped" { return .stopped }
        if status == "blocked" { return .blocked }
        if ["failed", "error", "cancelled", "canceled"].contains(status) { return .failed }
        if status == "completed" { return .completed }
        if awaiting && graphReady { return .awaitingConfirmation }
        if hasStartedTasks
            || ["confirmed", "processing", "running", "executing", "in_progress"].contains(status) {
            return .running
        }
        return .planning
    }

    private static func normalized(_ value: String?) -> String? {
        nonEmpty(value)?.lowercased()
    }

    private static func nonEmpty(_ value: String?) -> String? {
        let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? nil : trimmed
    }
}
