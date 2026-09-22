namespace ChatOS.Connector.AgentTeams;

internal static class AgentRequirementSurveySkillCatalog
{
    public static string Get(string scenario) => scenario switch
    {
        "create_survey" => CreateSurvey,
        "read_results" => ReadResults,
        "resolve_survey" => ResolveSurvey,
        "review_execution" => ReviewExecution,
        _ => throw new ArgumentOutOfRangeException(nameof(scenario), scenario,
            "Unknown requirement survey scenario."),
    };

    private const string CreateSurvey = """
        # 场景 Skill：创建需求调研

        目标是把会影响范围、方案、风险、时间或验收的 Human 决策，整理成一张不重复、可直接选择的 pending 调研单。

        1. 先提取已确认事实、尚未确认的决定和它影响的工作；不影响后续工作的临时问题不要创建调研。
        2. 调用 requirement_survey_list(status=pending)，对可能重复的候选逐一 requirement_survey_get。已有同主题 pending 调研或仍适用的历史决定时复用并退出。
        3. 一张单只处理一个主题；每题只问一个维度。single_choice 用于互斥决定，multiple_choice 用于可组合决定；每题提供 2–12 个具体且可执行的选项。页面统一提供备注，不创建自由文本题。
        4. 调用 requirement_survey_create。request_key、question key 和 option key 必须稳定、可读；结果不明确时只能用相同 request_key 和完全相同内容重试。
        5. 确认返回 project_bound=true 且状态为 pending，然后报告需要 Human 决定的内容并结束本轮；不要轮询或代替 Human 提交。
        """;

    private const string ReadResults = """
        # 场景 Skill：读取调研结果

        目标是从当前项目的真实调研记录中，区分 Human 选择、备注、既有方案和尚未确认事项。

        1. 刚提交的调研用 requirement_survey_list(status=submitted) 定位；回顾历史决定时可不传 status。
        2. 对候选调用 requirement_survey_get，逐项读取状态、问题、选项、选择结果、notes 和 resolution。
        3. pending 只表示仍待 Human 提交，不推断答案；submitted 且 resolution 为空表示决定已具备但正式方案尚未形成；resolution 非空时分别陈述 Human 决策和 Agent 方案。
        4. 输出 Human 已确认、备注补充、既有方案与执行步骤、尚未确认或冲突，以及使用的调研标题和状态。聊天摘要只能帮助定位，不能替代调研原始记录。
        """;

    private const string ResolveSurvey = """
        # 场景 Skill：生成解决方案与执行计划

        目标是把 Human 已提交的选择和备注转成边界明确、可实施、可验收的方案，并写回同一张调研。

        1. requirement_survey_list(status=submitted) 后立即 requirement_survey_get；只有 submitted 才继续。
        2. 把每道题的选择和备注映射到范围、方案、风险和验收条件。备注与选项冲突时列为待确认并停止写入。已有 resolution 且事实未变化时直接复用。
        3. summary 写最终决定和边界；solution_markdown 至少覆盖依据、方案、范围、非目标、关键设计、迁移或兼容策略与验证方式。
        4. execution_steps 按依赖顺序编排，每步写明动作、负责人建议、交付物和验收条件；计划不能宣称已经执行。
        5. 调用 requirement_survey_resolve 后再次 requirement_survey_get，确认 Human 限制和步骤都已保存。resolution 只表示规划完成，不表示实施完成。
        """;

    private const string ReviewExecution = """
        # 场景 Skill：核对方案执行进度

        目标是把正式执行计划与当前项目所有团队 Todo 的事实对照，说明已完成、进行中、阻塞、未覆盖和无法确认的部分。

        1. requirement_survey_list/get 定位有 resolution 的基准方案；没有 resolution 时报告缺少可核对计划并退出。
        2. 调用 requirement_survey_project_tasks 读取当前项目任务事实。
        3. 按目标、范围、交付物和验收条件匹配步骤与任务，不能只按标题。无法可靠匹配时标记未确认。
        4. 只有 Todo 为 Completed 且 Result 提供与验收条件相关的证据，步骤才算完成；InProgress 是执行中；Blocked 必须带出原因；没有对应 Todo 的步骤标记未覆盖。
        5. 分别输出总体进度、逐步证据、阻塞项、未覆盖项和下一任务建议；不要修改调研或 Todo 状态。
        """;
}
