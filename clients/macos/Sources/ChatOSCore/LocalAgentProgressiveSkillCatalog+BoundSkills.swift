import CryptoKit
import Foundation

/// An immutable, run-scoped view of the profession and project-type Skills bound by the client.
/// Only this snapshot is exposed to the model, so a resumed run cannot silently switch identity or
/// inherit edits made after the run started.
public struct LocalAgentProgressiveSkillSnapshot: Codable, Sendable, Equatable {
    public let schemaVersion: Int
    public let language: ChatOSLanguage
    public let skills: [LocalAgentBoundProgressiveSkill]

    public init(
        schemaVersion: Int = 1,
        language: ChatOSLanguage,
        skills: [LocalAgentBoundProgressiveSkill]
    ) {
        self.schemaVersion = schemaVersion
        self.language = language
        self.skills = skills
    }

    public var routerMarkdown: String {
        let heading = language == .english
            ? "## Bound profession and project Skills"
            : "## 当前绑定的职业与项目 Skill"
        let rule = language == .english
            ? "These are identity-bound Router entries, not the complete instructions. Before making specialist decisions, producing a deliverable, defining acceptance, or reporting completion, call `agent_skill_activate` for the relevant `skill_ref`. Read only the references needed for the current decision. You may not activate an unlisted Skill or switch profession/project type."
            : "以下只是身份绑定的 Router 目录，不是完整规则。进行专业判断、编写交付物、定义验收或声称完成前，必须先用 `agent_skill_activate` 激活相关 `skill_ref`；只按当前问题读取必要参考资料。不得激活目录外 Skill，也不得切换职业或项目类型。"
        let rows = skills.map {
            "- `\($0.skillRef)` · \($0.kind.routerLabel(language: language)) · **\($0.label)**：\($0.description)"
        }.joined(separator: "\n")
        return [heading, rule, rows].joined(separator: "\n\n")
    }
}

public enum LocalAgentProgressiveSkillKind: String, Codable, Sendable {
    case profession
    case projectType = "project_type"

    fileprivate func routerLabel(language: ChatOSLanguage) -> String {
        switch (self, language) {
        case (.profession, .english): "profession"
        case (.profession, _): "职业"
        case (.projectType, .english): "project type"
        case (.projectType, _): "项目类型"
        }
    }
}

public struct LocalAgentProgressiveSkillResource: Codable, Sendable, Equatable, Identifiable {
    public let relativePath: String
    public let title: String
    public let summary: String
    public let markdown: String
    public let contentSHA256: String

    public var id: String { relativePath }
    public var sizeBytes: Int { markdown.lengthOfBytes(using: .utf8) }

    public init(relativePath: String, title: String, summary: String, markdown: String) {
        self.relativePath = relativePath
        self.title = title
        self.summary = summary
        self.markdown = markdown
        contentSHA256 = Self.sha256(markdown)
    }

    private static func sha256(_ value: String) -> String {
        SHA256.hash(data: Data(value.utf8)).map { String(format: "%02x", $0) }.joined()
    }
}

public struct LocalAgentBoundProgressiveSkill: Codable, Sendable, Equatable, Identifiable {
    public let skillRef: String
    public let kind: LocalAgentProgressiveSkillKind
    public let key: String
    public let name: String
    public let label: String
    public let description: String
    public let category: String
    public let instructions: String
    public let instructionsSHA256: String
    public let resources: [LocalAgentProgressiveSkillResource]

    public var id: String { skillRef }

    public init(
        kind: LocalAgentProgressiveSkillKind,
        key: String,
        name: String,
        label: String,
        description: String,
        category: String,
        instructions: String,
        resources: [LocalAgentProgressiveSkillResource]
    ) {
        self.kind = kind
        self.key = key
        self.name = name
        self.label = label
        self.description = description
        self.category = category
        self.instructions = instructions.trimmingCharacters(in: .whitespacesAndNewlines)
        instructionsSHA256 = SHA256.hash(data: Data(self.instructions.utf8))
            .map { String(format: "%02x", $0) }.joined()
        let refDigest = String(instructionsSHA256.prefix(12))
        skillRef = "AS-\(kind.rawValue)-\(key)-\(refDigest)"
        self.resources = resources
    }
}

/// Builds the detailed leaf resources shared by runtime tools and the Skill management UI.
/// Every catalog entry receives the same disclosure contract while retaining its own label,
/// description, category and editable main instructions.
public extension LocalAgentProgressiveSkillCatalog {
    static func boundSnapshot(
        profession: LocalAgentProfessionDefinition,
        projectType: LocalProjectTypeDefinition?,
        language: ChatOSLanguage
    ) -> LocalAgentProgressiveSkillSnapshot {
        var skills = [boundProfessionSkill(profession, language: language)]
        if let projectType {
            skills.append(boundProjectTypeSkill(projectType, language: language))
        }
        return .init(language: language, skills: skills)
    }

    static func boundProfessionSkill(
        _ value: LocalAgentProfessionDefinition,
        language: ChatOSLanguage
    ) -> LocalAgentBoundProgressiveSkill {
        let localized = LocalizedSkill(
            kind: .profession,
            key: value.key,
            name: value.chatOSSkillName,
            label: language == .english ? value.labelEN : value.label,
            description: language == .english ? value.descriptionEN : value.description,
            category: language == .english ? value.categoryLabelEN : value.categoryLabel,
            instructions: language == .english ? value.skillMarkdownEN : value.skillMarkdown,
            language: language
        )
        return localized.build()
    }

    static func boundProjectTypeSkill(
        _ value: LocalProjectTypeDefinition,
        language: ChatOSLanguage
    ) -> LocalAgentBoundProgressiveSkill {
        let localized = LocalizedSkill(
            kind: .projectType,
            key: value.key,
            name: value.skillName,
            label: language == .english ? value.labelEN : value.label,
            description: language == .english ? value.descriptionEN : value.description,
            category: language == .english ? value.categoryLabelEN : value.categoryLabel,
            instructions: language == .english ? value.ruleMarkdownEN : value.ruleMarkdown,
            language: language
        )
        return localized.build()
    }
}

private struct LocalizedSkill {
    let kind: LocalAgentProgressiveSkillKind
    let key: String
    let name: String
    let label: String
    let description: String
    let category: String
    let instructions: String
    let language: ChatOSLanguage

    func build() -> LocalAgentBoundProgressiveSkill {
        .init(
            kind: kind,
            key: key,
            name: name,
            label: label,
            description: description,
            category: category,
            instructions: instructions,
            resources: language == .english ? englishResources : chineseResources
        )
    }

    private var chineseResources: [LocalAgentProgressiveSkillResource] {
        [
            resource(
                path: "references/workflow.md",
                title: "工作流与决策路径",
                summary: "从接收目标、澄清事实、拆分工作到验证与交接的详细步骤。",
                body: """
                # \(label)：工作流与决策路径

                ## 适用目标

                本 Skill 绑定为“\(label)”（分类：\(category)）。核心职责是：\(description)。使用本页时先确认当前请求确实落在这一职责内；若涉及另一职业的独立判断，只陈述接口、依赖和所需证据，不假装拥有另一身份或权限。

                ## 开始前

                1. 从 Human 消息、当前 Todo、团队目标和已读取资产中提取目标、范围、约束、截止条件与明确排除项。
                2. 将事实、假设、未知项分开。事实必须能追溯到消息、代码、数据、工具输出或已确认决策；假设必须标注验证办法。
                3. 确认执行权限和工具能力。Skill 只提供方法，不授予写入、发布、创建任务、访问私密数据或代表 Human 决策的权限。
                4. 为交付物写出可观察的验收条件，包含正常路径、关键边界、失败与恢复路径。

                ## 执行循环

                - 建立最小可验证计划：每一步都说明输入、动作、产物、验证与失败后的处理。
                - 优先读取真实现状，再做设计或修改；避免用模板覆盖已有约定。
                - 进行中的重要发现及时写入任务进度，区分“已做”“已观察”“待确认”和“下一步”。
                - 发现范围扩大、不可逆影响、权限不足、证据冲突或外部依赖时暂停相关动作并升级，不用猜测填补。
                - 完成前回到原始目标逐条核验，不能用“代码已写”“文档已生成”代替结果已经生效。

                ## 交接

                交接至少包含结论、变更范围、证据位置、验证方法、残余风险、未解决问题和下一责任人。项目类型 Skill 同时存在时，还要读取它的工作流资料，用职业方法完成项目形态要求；两者冲突时以真实权限、Human 指令和更严格的验收门槛为准。
                """
            ),
            resource(
                path: "references/deliverables-and-evidence.md",
                title: "交付物与证据",
                summary: "定义什么可以交付、如何引用证据以及何时才允许声称完成。",
                body: """
                # \(label)：交付物与证据

                ## 交付物契约

                围绕“\(description)”选择最小但完整的交付集合。每项交付物必须写清受众、用途、存放位置、版本或时间点、所有者和验收方式。可以是实现、配置、设计、分析、运行记录、决策、数据集、文档或操作结果，但不能只给没有验证上下文的摘要。

                ## 证据等级

                1. **直接证据**：测试输出、工具返回、可复现命令、查询结果、截图、运行日志、评审记录或 Human 明确确认。
                2. **可追溯证据**：代码/文档路径、版本、输入数据范围、环境与时间，足以让下一位执行者复核。
                3. **推断**：基于事实的分析，但尚未通过真实环境或负责人确认。必须标为推断，不能混入完成结论。

                ## 完成声明检查表

                - 原始目标与明确验收条件是否逐条对应到证据。
                - 关键失败路径、边界条件、权限和数据影响是否验证。
                - 是否说明未测试范围、环境差异、样本偏差或仍需 Human 决策的事项。
                - 若结果需要部署、发布、审批、迁移或运营接管，是否真的完成该步骤；否则只能写“已准备”“待审批”或“待上线”。
                - 证据是否来自当前版本与当前运行，而非旧结论、猜测或无关项目。

                ## 推荐交付摘要

                用“结论 → 已完成范围 → 关键证据 → 风险/限制 → 下一步与负责人”的顺序。遇到失败要保留现场、错误信息和已尝试动作；遇到阻塞要说明解除条件，而不是将部分产出包装成完成。
                """
            ),
            resource(
                path: "references/quality-gates-and-risks.md",
                title: "质量门禁与风险",
                summary: "给出适用于当前 Skill 的验证门禁、常见失败模式和风险处理。",
                body: """
                # \(label)：质量门禁与风险

                ## 必过门禁

                - **正确性**：产物满足已确认目标，并有正向、边界和失败场景证据。
                - **一致性**：术语、接口、数据、状态和决策与项目现状一致，不制造第二套真相。
                - **可复核性**：另一位成员能依据记录重现关键判断与验证结果。
                - **安全与权限**：敏感信息、写入范围、外部通信和不可逆动作均在授权边界内。
                - **可运营性**：明确监控、回滚/恢复、维护责任和遗留风险；不把一次成功当成长期可靠。

                ## 常见失败模式

                对“\(description)”尤其要防止：只复述需求而未核实现状；为了显得完整而虚构事实；只验证顺利路径；遗漏受影响的上下游；将建议写成已执行；以篇幅替代清晰验收；发现证据冲突后仍继续推进。

                ## 风险分级

                - 低风险：可逆、局部、证据充分，可在当前范围内修正并记录。
                - 中风险：影响多个成员、接口、数据或时间，需要同步项目经理/相关负责人后继续。
                - 高风险：可能造成数据丢失、安全/合规问题、生产中断、对外承诺或不可逆成本，必须停止相关动作并请求 Human 决策。

                ## 退出条件

                只有在交付物、验证证据、已知限制和交接责任均清楚时才能退出。若任何验收项未完成，状态应保持进行中、受阻或待审核，并写明下一次可执行动作与所需输入。
                """
            ),
            resource(
                path: "references/collaboration-and-escalation.md",
                title: "协作、边界与升级",
                summary: "明确与项目经理、其他职业 Agent 和 Human 的协作接口。",
                body: """
                # \(label)：协作、边界与升级

                ## 职责边界

                当前身份负责：\(description)。职业 Skill 不会扩大客户端提供的工具与数据权限，也不会赋予项目经理身份。只有明确绑定的项目经理维护团队任务结构和项目看板；普通成员通过消息与任务进度提供事实、建议、风险和交付证据。

                ## 协作请求应包含

                - 背景和要解决的问题，而非只抛出一句动作指令。
                - 期望输出、验收条件、截止/优先级、输入位置和已知约束。
                - 已完成的调查、证据与失败尝试，避免对方重复劳动。
                - 需要对方做出的明确决定，以及没有回复时受影响的范围。

                ## 必须升级的情况

                目标或验收互相矛盾；需要越权访问或不可逆操作；关键输入缺失且不同假设会改变方案；安全、隐私、合规或生产稳定性风险；跨团队依赖没有负责人；范围或成本显著变化；真实结果与项目看板状态不一致。

                ## 沟通格式

                日常同步保持短而可执行：先给结论，再给证据、影响和下一步。复杂分析放在可追溯文档或共享资产中，消息只给摘要与链接。需要 Human 决策时列出选项、取舍、推荐项和最晚决策点；不要把内部推理、模糊状态或工具细节堆给 Human。
                """
            ),
            LocalAgentProgressiveSkillExampleCatalog.resource(
                kind: kind,
                key: key,
                label: label,
                language: language
            ),
        ]
    }

    private var englishResources: [LocalAgentProgressiveSkillResource] {
        [
            resource(path: "references/workflow.md", title: "Workflow and decision path", summary: "Detailed path from intake and fact finding through execution, validation, and handoff.", body: """
            # \(label): workflow and decision path

            This Skill is bound as **\(label)** in **\(category)**. Its purpose is: \(description). First confirm that the request belongs to this responsibility. For another profession's independent judgment, describe the interface, dependency, and required evidence without claiming that identity or authority.

            ## Before work

            1. Extract the objective, scope, constraints, deadline, and exclusions from Human messages, the current Todo, team goals, and assets already read.
            2. Separate facts, assumptions, and unknowns. Facts must trace to messages, code, data, tool output, or approved decisions; every assumption needs a validation path.
            3. Confirm authority and available tools. A Skill supplies method, never permission to write, publish, create tasks, access private data, or decide on behalf of a Human.
            4. Define observable acceptance for the normal path, critical boundaries, failure behavior, and recovery.

            ## Execution loop

            Use the smallest verifiable plan. For each step record input, action, output, verification, and failure handling. Read the real current state before designing or editing. Report material discoveries as facts, observations, open questions, and next actions. Stop and escalate scope growth, irreversible impact, missing authority, conflicting evidence, or unmanaged external dependencies. Before completion, map every original objective to current evidence; implementation or document creation alone does not prove the outcome is live.

            ## Handoff

            Provide conclusion, changed scope, evidence locations, reproduction or validation steps, residual risk, open questions, and next owner. When a project-type Skill is also bound, use this profession method to satisfy that project's delivery shape and read its workflow when relevant. Real authority, Human direction, and the stricter acceptance gate always win.
            """),
            resource(path: "references/deliverables-and-evidence.md", title: "Deliverables and evidence", summary: "Defines acceptable outputs, evidence traceability, and completion claims.", body: """
            # \(label): deliverables and evidence

            Build the smallest complete deliverable set for “\(description).” For every output state its audience, purpose, location, version or time boundary, owner, and acceptance method. Outputs may be implementation, configuration, design, analysis, run records, decisions, data, documentation, or operational results; an unverified summary is not a deliverable.

            ## Evidence ladder

            1. **Direct evidence:** tests, tool results, reproducible commands, queries, screenshots, logs, reviews, or explicit Human confirmation.
            2. **Traceable evidence:** paths, versions, input ranges, environment, and time sufficient for another person to verify.
            3. **Inference:** reasoned interpretation not yet validated in the real environment. Label it as inference and keep it out of completion claims.

            ## Completion gate

            Map every acceptance criterion to evidence; cover boundaries, failure paths, permissions, and data impact; disclose untested scope and environment differences. If deployment, approval, migration, publication, or operational takeover is still pending, say “prepared” or “awaiting approval,” not “complete.” Evidence must come from the current version and run. Summarize as conclusion, completed scope, evidence, risks or limits, and next owner. Preserve failure context and state the condition that unblocks blocked work.
            """),
            resource(path: "references/quality-gates-and-risks.md", title: "Quality gates and risks", summary: "Verification gates, common failure modes, and risk handling for this Skill.", body: """
            # \(label): quality gates and risks

            Required gates are correctness against confirmed objectives; consistency with project terminology, interfaces, data, states, and decisions; reproducibility by another contributor; authorization for sensitive data, external communication, and irreversible actions; and operability through monitoring, recovery, ownership, and documented residual risk.

            For “\(description),” watch for restating the request without inspecting reality, inventing facts to make an answer look complete, testing only the happy path, missing downstream consumers, describing a recommendation as executed, substituting length for acceptance, or continuing after evidence conflicts.

            Treat reversible local issues with strong evidence as low risk. Multi-owner, interface, data, or schedule impact is medium risk and needs coordination. Potential data loss, security or compliance exposure, production interruption, external commitments, or irreversible cost is high risk: stop the affected action and request Human direction.

            Exit only when deliverables, evidence, limitations, and ownership are clear. Otherwise retain an in-progress, blocked, or review state and name the next executable action and required input.
            """),
            resource(path: "references/collaboration-and-escalation.md", title: "Collaboration and escalation", summary: "Interfaces with the Project Manager, other professions, and the Human.", body: """
            # \(label): collaboration and escalation

            This identity owns: \(description). The Skill does not expand tool or data authority and does not confer Project Manager status. Only the explicitly bound Project Manager maintains team task structure and the project dashboard. Other members contribute facts, recommendations, risks, progress, and delivery evidence through messages and Todo progress.

            A useful collaboration request includes context and problem, expected output and acceptance, priority or deadline, input locations, constraints, prior investigation, failed attempts, the exact decision needed, and the consequence of delay. Escalate contradictory goals, unauthorized or irreversible actions, missing inputs where assumptions change the design, security/privacy/compliance/production risk, ownerless cross-team dependencies, material scope or cost change, and disagreement between real evidence and dashboard state.

            Keep routine updates concise and actionable: conclusion, evidence, impact, next step. Put complex analysis in a traceable document or shared asset and link it from the message. For a Human decision, present options, trade-offs, a recommendation, and the latest decision point rather than internal reasoning or tool noise.
            """),
            LocalAgentProgressiveSkillExampleCatalog.resource(
                kind: kind,
                key: key,
                label: label,
                language: language
            ),
        ]
    }

    private func resource(
        path: String,
        title: String,
        summary: String,
        body: String
    ) -> LocalAgentProgressiveSkillResource {
        .init(
            relativePath: path,
            title: title,
            summary: summary,
            markdown: body.trimmingCharacters(in: .whitespacesAndNewlines)
        )
    }
}
