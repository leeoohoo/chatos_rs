import ChatOSCore
import Foundation

struct TerminalRequestDTO: Encodable {
    var workspaceID: String
    var command: String
    var args: [String]
    var cwd: String?
    var timeoutMilliseconds: Int
    enum CodingKeys: String, CodingKey {
        case workspaceID = "workspace_id"
        case command, args, cwd
        case timeoutMilliseconds = "timeout_ms"
    }
}

struct TerminalResultDTO: Decodable, Sendable {
    var command: String
    var args: [String]
    var cwd: String
    var success: Bool
    var exitCode: Int?
    var timedOut: Bool
    var stdout: String
    var stderr: String
    var error: String?
    enum CodingKeys: String, CodingKey {
        case command, args, cwd, success, stdout, stderr, error
        case exitCode = "exit_code"
        case timedOut = "timed_out"
    }
    var domainModel: LocalConnectorTerminalResult {
        .init(
            command: command, args: args, cwd: cwd, success: success,
            exitCode: exitCode, timedOut: timedOut, stdout: stdout, stderr: stderr, error: error
        )
    }
}

struct CommandHistoryDTO: Decodable, Sendable {
    var entries: [CommandHistoryEntryDTO]
}

struct CommandHistoryEntryDTO: Decodable, Sendable {
    var id: String
    var source: String
    var workspaceAlias: String?
    var cwd: String?
    var display: String
    var status: String
    var exitCode: Int?
    var stdoutPreview: String?
    var stderrPreview: String?
    var error: String?
    var startedAt: String
    enum CodingKeys: String, CodingKey {
        case id, source, cwd, display, status, error
        case workspaceAlias = "workspace_alias"
        case exitCode = "exit_code"
        case stdoutPreview = "stdout_preview"
        case stderrPreview = "stderr_preview"
        case startedAt = "started_at"
    }
    var domainModel: LocalConnectorCommandHistoryEntry {
        .init(
            id: id, source: source, workspaceAlias: workspaceAlias, cwd: cwd,
            display: display, status: status, exitCode: exitCode,
            stdoutPreview: stdoutPreview, stderrPreview: stderrPreview,
            error: error, startedAt: startedAt
        )
    }
}

struct ApprovalSettingsDTO: Decodable, Sendable {
    var defaultMode: LocalConnectorApprovalMode
    var history: [ApprovalHistoryEntryDTO]
    enum CodingKeys: String, CodingKey {
        case defaultMode = "default_mode"
        case history
    }
    var domainModel: LocalConnectorApprovalSettings {
        .init(defaultMode: defaultMode, history: history.map(\.domainModel))
    }
}

struct ApprovalHistoryEntryDTO: Decodable, Sendable {
    var id: String
    var command: String
    var cwd: String
    var source: String
    var mode: LocalConnectorApprovalMode
    var decision: String
    var risk: String
    var reason: String?
    var createdAt: String
    enum CodingKeys: String, CodingKey {
        case id, command, cwd, source, mode, decision, risk, reason
        case createdAt = "created_at"
    }
    var domainModel: LocalConnectorApprovalHistoryEntry {
        .init(
            id: id, command: command, cwd: cwd, source: source,
            mode: mode, decision: decision, risk: risk, reason: reason, createdAt: createdAt
        )
    }
}

struct UpdateApprovalSettingsDTO: Encodable {
    var defaultMode: String
    var riskAcknowledged: Bool
    enum CodingKeys: String, CodingKey {
        case defaultMode = "default_mode"
        case riskAcknowledged = "risk_acknowledged"
    }
}

struct PendingApprovalsDTO: Decodable, Sendable {
    var items: [PendingApprovalDTO]
    var reviewing: [PendingApprovalDTO]
    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        items = try container.decodeIfPresent([PendingApprovalDTO].self, forKey: .items) ?? []
        reviewing = try container.decodeIfPresent([PendingApprovalDTO].self, forKey: .reviewing) ?? []
    }
    enum CodingKeys: String, CodingKey { case items, reviewing }
}

struct PendingApprovalDTO: Decodable, Sendable {
    var id: String
    var requestID: String
    var command: String
    var cwd: String
    var source: String
    var risk: String
    var reason: String?
    var createdAt: String
    var availableDecisions: [String]
    enum CodingKeys: String, CodingKey {
        case id, command, cwd, source, risk, reason
        case requestID = "request_id"
        case createdAt = "created_at"
        case availableDecisions = "available_decisions"
    }
    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        requestID = try container.decode(String.self, forKey: .requestID)
        command = try container.decode(String.self, forKey: .command)
        cwd = try container.decode(String.self, forKey: .cwd)
        source = try container.decode(String.self, forKey: .source)
        risk = try container.decode(String.self, forKey: .risk)
        reason = try container.decodeIfPresent(String.self, forKey: .reason)
        createdAt = try container.decode(String.self, forKey: .createdAt)
        availableDecisions = try container.decodeIfPresent([String].self, forKey: .availableDecisions) ?? []
    }
    var domainModel: LocalConnectorPendingApproval {
        .init(
            id: id, requestID: requestID, command: command, cwd: cwd,
            source: source, risk: risk, reason: reason, createdAt: createdAt,
            availableDecisions: availableDecisions
        )
    }
}

struct ApproveApprovalDTO: Encodable {
    var decision: String
    var rememberAllow: Bool
    var riskAcknowledged: Bool
    enum CodingKeys: String, CodingKey {
        case decision
        case rememberAllow = "remember_allow"
        case riskAcknowledged = "risk_acknowledged"
    }
}

struct DenyApprovalDTO: Encodable { var reason: String }
