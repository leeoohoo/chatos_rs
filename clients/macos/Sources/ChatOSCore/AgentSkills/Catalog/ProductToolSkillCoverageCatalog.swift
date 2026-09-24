import Foundation

public enum ProductToolProviderID {
    public static let agentBuilder = "chatos.auxiliary.agent-builder"
    public static let commandApproval = "chatos.auxiliary.command-approval"
    public static let capabilityBroker = "chatos.local.capability-broker"
    public static let localAgentChat = "chatos.local.agent-chat"
    public static let localProjectTeam = "chatos.local.project-team"
    public static let projectRead = "chatos.builtin.project-read"
    public static let projectWrite = "chatos.builtin.project-write"
    public static let remoteConnection = "chatos.builtin.remote-connection"
    public static let terminal = "chatos.builtin.terminal"
    public static let requirementSurvey = "chatos.builtin.requirement-survey"
}

public enum ProductToolSkillBindingID {
    public static let agentBuilder = "auxiliary.agent-builder"
    public static let commandApproval = "auxiliary.command-approval"
    public static let capabilityBroker = "capability-broker.control-plane"
    public static let agentSkillControlPlane = "agent-chat.skill-control-plane"
    public static let relayContext = "agent-chat.relay-context"
    public static let collaborationMessaging = "agent-chat.collaboration-messaging"
    public static let agentStaffing = "agent-chat.staffing"
    public static let todoPlanning = "agent-chat.todo-planning"
    public static let todoExecution = "agent-chat.todo-execution"
    public static let teamKnowledge = "agent-chat.team-knowledge"
    public static let projectDashboard = "agent-chat.project-dashboard"
    public static let projectTeamCatalog = "project-team.catalog"
    public static let projectTeamProposal = "project-team.proposal"
    public static let projectRead = "project-files.read"
    public static let projectWrite = "project-files.write"
    public static let remoteConnection = "remote-connection.operations"
    public static let terminalCommandExecution = "terminal.command-execution"
    public static let terminalProcessObservation = "terminal.process-observation"
    public static let terminalProcessControl = "terminal.process-control"
    public static let requirementSurveyControlPlane = "requirement-survey.control-plane"
    public static let requirementSurveyCreate = "requirement-survey.create"
    public static let requirementSurveyReadResults = "requirement-survey.read-results"
    public static let requirementSurveyResolve = "requirement-survey.resolve"
    public static let requirementSurveyReviewExecution = "requirement-survey.review-execution"
}

public extension ToolSkillCoverageCatalog {
    static let product: ToolSkillCoverageCatalog = {
        do {
            return try .init(bindings: [
                .init(
                    id: ProductToolSkillBindingID.agentBuilder,
                    providerID: ProductToolProviderID.agentBuilder,
                    toolNames: [
                        "project_inspect", "model_list", "profession_list", "agent_draft",
                    ],
                    routerSkillName: "chatos-agent-builder",
                    specialistSkillName: "chatos-agent-builder",
                    activationPolicy: .runBound
                ),
                .init(
                    id: ProductToolSkillBindingID.commandApproval,
                    providerID: ProductToolProviderID.commandApproval,
                    toolNames: [
                        "read_file_raw", "read_file_range", "list_dir", "search_text",
                        "approval_decision",
                    ],
                    routerSkillName: "chatos-command-approval",
                    specialistSkillName: "chatos-command-approval",
                    activationPolicy: .runBound
                ),
                .init(
                    id: ProductToolSkillBindingID.capabilityBroker,
                    providerID: ProductToolProviderID.capabilityBroker,
                    toolNames: [
                        "capability_search", "capability_describe", "capability_skill_activate",
                        "capability_skill_read_resource", "capability_invoke",
                    ],
                    routerSkillName: "chatos-capability-discovery",
                    specialistSkillName: "chatos-capability-discovery",
                    activationPolicy: .runBound
                ),
                .init(
                    id: ProductToolSkillBindingID.remoteConnection,
                    providerID: ProductToolProviderID.remoteConnection,
                    toolNames: [
                        "test_connection", "run_command", "list_directory", "read_file",
                        "download_file", "upload_file",
                    ],
                    routerSkillName: "chatos-remote-connection",
                    specialistSkillName: "chatos-remote-connection",
                    activationPolicy: .runBound
                ),
                .init(
                    id: ProductToolSkillBindingID.agentSkillControlPlane,
                    providerID: ProductToolProviderID.localAgentChat,
                    toolNames: [
                        "agent_skill_activate", "agent_skill_list_resources",
                        "agent_skill_read_resource",
                    ],
                    routerSkillName: "chatos-skill-runtime",
                    specialistSkillName: "chatos-skill-runtime",
                    activationPolicy: .controlPlane
                ),
                .init(
                    id: ProductToolSkillBindingID.relayContext,
                    providerID: ProductToolProviderID.localAgentChat,
                    toolNames: [
                        "relay_bootstrap", "agent_workspace_snapshot", "chat_get_trigger",
                        "chat_list_members", "chat_read_unread", "chat_read_all_unread",
                        "chat_read_messages", "chat_read_attachment",
                    ],
                    routerSkillName: "chatos-relay-context",
                    specialistSkillName: "chatos-relay-context",
                    activationPolicy: .runBound
                ),
                .init(
                    id: ProductToolSkillBindingID.collaborationMessaging,
                    providerID: ProductToolProviderID.localAgentChat,
                    toolNames: [
                        "chat_inbox_send", "chat_document_create", "chat_mark_read",
                        "chat_direct_open", "chat_direct_send", "chat_team_send",
                        "chat_send_message", "chat_heartbeat_complete", "agent_cycle_complete",
                    ],
                    routerSkillName: "chatos-collaboration-messaging",
                    specialistSkillName: "chatos-collaboration-messaging",
                    activationPolicy: .runBound
                ),
                .init(
                    id: ProductToolSkillBindingID.agentStaffing,
                    providerID: ProductToolProviderID.localAgentChat,
                    toolNames: [
                        "agent_propose_member", "agent_propose_existing_member",
                        "agent_propose_member_removal",
                    ],
                    routerSkillName: "chatos-agent-staffing",
                    specialistSkillName: "chatos-agent-staffing"
                ),
                .init(
                    id: ProductToolSkillBindingID.todoPlanning,
                    providerID: ProductToolProviderID.localAgentChat,
                    toolNames: [
                        "todo_list", "todo_schedule_state", "todo_start_next", "todo_add",
                        "todo_update", "todo_reorder", "todo_execution_options",
                        "todo_dependency_options",
                    ],
                    routerSkillName: "chatos-todo-planning",
                    specialistSkillName: "chatos-todo-planning"
                ),
                .init(
                    id: ProductToolSkillBindingID.todoExecution,
                    providerID: ProductToolProviderID.localAgentChat,
                    toolNames: [
                        "todo_get_context", "todo_progress_append", "todo_read_progress",
                        "todo_complete", "todo_block",
                    ],
                    routerSkillName: "chatos-todo-execution",
                    specialistSkillName: "chatos-todo-execution",
                    activationPolicy: .runBound
                ),
                .init(
                    id: ProductToolSkillBindingID.teamKnowledge,
                    providerID: ProductToolProviderID.localAgentChat,
                    toolNames: [
                        "team_asset_list", "team_asset_get", "team_asset_create",
                        "team_asset_update", "team_asset_archive",
                    ],
                    routerSkillName: "chatos-team-knowledge",
                    specialistSkillName: "chatos-team-knowledge"
                ),
                .init(
                    id: ProductToolSkillBindingID.projectDashboard,
                    providerID: ProductToolProviderID.localAgentChat,
                    toolNames: ["project_dashboard_get", "project_dashboard_update"],
                    routerSkillName: "chatos-project-dashboard",
                    specialistSkillName: "chatos-project-dashboard"
                ),
                .init(
                    id: ProductToolSkillBindingID.projectTeamCatalog,
                    providerID: ProductToolProviderID.localProjectTeam,
                    toolNames: ["project_catalog"],
                    routerSkillName: "chatos-skill-runtime",
                    specialistSkillName: "chatos-skill-runtime",
                    activationPolicy: .controlPlane
                ),
                .init(
                    id: ProductToolSkillBindingID.projectTeamProposal,
                    providerID: ProductToolProviderID.localProjectTeam,
                    toolNames: [
                        "team_propose_existing", "team_propose_new_project",
                        "team_propose_import_directory",
                    ],
                    routerSkillName: "chatos-project-team-setup",
                    specialistSkillName: "chatos-project-team-setup"
                ),
                .init(
                    id: ProductToolSkillBindingID.projectRead,
                    providerID: ProductToolProviderID.projectRead,
                    toolNames: [
                        "read_file_raw", "read_file_range", "list_dir", "search_text",
                        "read_file", "search_files", "open_file_in_pet",
                    ],
                    routerSkillName: "chatos-project-files",
                    specialistSkillName: "chatos-project-read"
                ),
                .init(
                    id: ProductToolSkillBindingID.projectWrite,
                    providerID: ProductToolProviderID.projectWrite,
                    toolNames: [
                        "open_edit_session", "stage_edit_batch", "commit_edit_session",
                        "abort_edit_session",
                    ],
                    routerSkillName: "chatos-project-files",
                    specialistSkillName: "chatos-project-write"
                ),
                .init(
                    id: ProductToolSkillBindingID.terminalCommandExecution,
                    providerID: ProductToolProviderID.terminal,
                    toolNames: ["execute_command"],
                    routerSkillName: "chatos-terminal",
                    specialistSkillName: "chatos-terminal-command-execution"
                ),
                .init(
                    id: ProductToolSkillBindingID.terminalProcessObservation,
                    providerID: ProductToolProviderID.terminal,
                    toolNames: [
                        "get_recent_logs", "process_list", "process_poll", "process_log",
                        "process_wait",
                    ],
                    routerSkillName: "chatos-terminal",
                    specialistSkillName: "chatos-terminal-process-observation"
                ),
                .init(
                    id: ProductToolSkillBindingID.terminalProcessControl,
                    providerID: ProductToolProviderID.terminal,
                    toolNames: ["process_write", "process_kill", "process"],
                    routerSkillName: "chatos-terminal",
                    specialistSkillName: "chatos-terminal-process-control"
                ),
                .init(
                    id: ProductToolSkillBindingID.requirementSurveyControlPlane,
                    providerID: ProductToolProviderID.requirementSurvey,
                    toolNames: [
                        "skill_activate", "skill_list_resources", "skill_read_resource",
                    ],
                    routerSkillName: "requirement-survey",
                    specialistSkillName: "requirement-survey",
                    activationPolicy: .controlPlane
                ),
                .init(
                    id: ProductToolSkillBindingID.requirementSurveyCreate,
                    providerID: ProductToolProviderID.requirementSurvey,
                    toolNames: ["requirement_survey_create"],
                    routerSkillName: "requirement-survey",
                    specialistSkillName: "requirement-survey-create"
                ),
                .init(
                    id: ProductToolSkillBindingID.requirementSurveyReadResults,
                    providerID: ProductToolProviderID.requirementSurvey,
                    toolNames: ["requirement_survey_list", "requirement_survey_get"],
                    routerSkillName: "requirement-survey",
                    specialistSkillName: "requirement-survey-read-results"
                ),
                .init(
                    id: ProductToolSkillBindingID.requirementSurveyResolve,
                    providerID: ProductToolProviderID.requirementSurvey,
                    toolNames: ["requirement_survey_resolve"],
                    routerSkillName: "requirement-survey",
                    specialistSkillName: "requirement-survey-resolve"
                ),
                .init(
                    id: ProductToolSkillBindingID.requirementSurveyReviewExecution,
                    providerID: ProductToolProviderID.requirementSurvey,
                    toolNames: ["requirement_survey_project_tasks"],
                    routerSkillName: "requirement-survey",
                    specialistSkillName: "requirement-survey-review-execution"
                ),
            ])
        } catch {
            fatalError("Invalid product Tool-to-Skill coverage catalog: \(error)")
        }
    }()
}
