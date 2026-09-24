import Foundation

public enum ProductToolProviderID {
    public static let projectRead = "chatos.builtin.project-read"
    public static let projectWrite = "chatos.builtin.project-write"
    public static let terminal = "chatos.builtin.terminal"
    public static let requirementSurvey = "chatos.builtin.requirement-survey"
}

public enum ProductToolSkillBindingID {
    public static let terminalCommandExecution = "terminal.command-execution"
    public static let terminalProcessObservation = "terminal.process-observation"
    public static let terminalProcessControl = "terminal.process-control"
}

public extension ToolSkillCoverageCatalog {
    static let product: ToolSkillCoverageCatalog = {
        do {
            return try .init(bindings: [
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
            ])
        } catch {
            fatalError("Invalid product Tool-to-Skill coverage catalog: \(error)")
        }
    }()
}
