import Foundation

public enum ProductToolProviderID {
    public static let projectRead = "chatos.builtin.project-read"
    public static let projectWrite = "chatos.builtin.project-write"
    public static let terminal = "chatos.builtin.terminal"
    public static let requirementSurvey = "chatos.builtin.requirement-survey"
}

public enum ProductToolSkillBindingID {
    public static let projectRead = "project-files.read"
    public static let projectWrite = "project-files.write"
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
