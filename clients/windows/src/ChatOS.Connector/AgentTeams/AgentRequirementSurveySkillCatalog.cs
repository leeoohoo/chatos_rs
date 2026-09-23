using System.Security.Cryptography;
using System.Text;

namespace ChatOS.Connector.AgentTeams;

internal static class AgentRequirementSurveySkillCatalog
{
    internal sealed record Skill(
        string Ref,
        string Name,
        string Description,
        string Role,
        string Instructions,
        IReadOnlyDictionary<string, string> Resources);

    private static readonly IReadOnlyList<Skill> Skills =
    [
        new("SKreq-router", "requirement-survey",
            "判断何时需要需求调研，并路由到创建、读取、方案或执行核对 Skill。",
            "router", Router, new Dictionary<string, string>()),
        new("SKreq-create", "requirement-survey-create",
            "查重后创建一张项目级选择式需求调研。", "leaf", CreateSurvey,
            new Dictionary<string, string> { ["references/example.md"] = CreateExample }),
        new("SKreq-read", "requirement-survey-read-results",
            "读取并区分 Human 选择、备注、既有方案与未确认事项。", "leaf",
            ReadResults,
            new Dictionary<string, string> { ["references/example.md"] = ReadExample }),
        new("SKreq-resolve", "requirement-survey-resolve",
            "把 Human 提交结果转成有边界的方案和执行计划。", "leaf",
            ResolveSurvey,
            new Dictionary<string, string> { ["references/example.md"] = ResolveExample }),
        new("SKreq-review", "requirement-survey-review-execution",
            "把调研执行计划与项目 Todo 事实逐项核对。", "leaf",
            ReviewExecution,
            new Dictionary<string, string> { ["references/example.md"] = ReviewExample }),
    ];

    public static IReadOnlyList<object> Catalog() => Skills.Select(value => (object)new
    {
        skill_ref = value.Ref,
        value.Name,
        value.Description,
        value.Role,
    }).ToArray();

    public static object Activate(string skillRef)
    {
        var skill = Find(skillRef);
        return new
        {
            activated = true,
            skill_ref = skill.Ref,
            skill.Name,
            skill.Role,
            instructions = skill.Instructions,
            instructions_sha256 = Hash(skill.Instructions),
            resources = DescribeResources(skill),
        };
    }

    public static object ListResources(string skillRef)
    {
        var skill = Find(skillRef);
        return new { skill_ref = skill.Ref, resources = DescribeResources(skill) };
    }

    public static object ReadResource(
        string skillRef,
        string relativePath,
        int offset,
        int maximumCharacters)
    {
        var skill = Find(skillRef);
        var path = NormalizePath(relativePath);
        if (!skill.Resources.TryGetValue(path, out var content))
            throw new KeyNotFoundException("Skill resource was not found.");
        var characters = content.EnumerateRunes().Select(value => value.ToString()).ToArray();
        if (offset < 0 || offset > characters.Length || maximumCharacters is < 1 or > 64_000)
            throw new ArgumentOutOfRangeException(nameof(offset));
        var length = Math.Min(maximumCharacters, characters.Length - offset);
        var nextOffset = offset + length < characters.Length ? offset + length : (int?)null;
        return new
        {
            skill_ref = skill.Ref,
            relative_path = path,
            sha256 = Hash(content),
            content = string.Concat(characters.Skip(offset).Take(length)),
            offset,
            next_offset = nextOffset,
            truncated = nextOffset is not null,
        };
    }

    private static Skill Find(string skillRef) =>
        Skills.FirstOrDefault(value => string.Equals(value.Ref, skillRef,
            StringComparison.Ordinal)) ?? throw new KeyNotFoundException("Unknown Skill reference.");

    private static IReadOnlyList<object> DescribeResources(Skill skill) =>
        skill.Resources.OrderBy(value => value.Key, StringComparer.Ordinal).Select(value => (object)new
        {
            relative_path = value.Key,
            kind = value.Key.StartsWith("references/", StringComparison.Ordinal)
                ? "reference" : "other",
            size_bytes = Encoding.UTF8.GetByteCount(value.Value),
            sha256 = Hash(value.Value),
        }).ToArray();

    private static string NormalizePath(string value)
    {
        var path = value.Trim();
        if (path.StartsWith("./", StringComparison.Ordinal)) path = path[2..];
        var parts = path.Split('/');
        if (string.IsNullOrEmpty(path) || path.StartsWith("/", StringComparison.Ordinal) ||
            parts.Any(part => string.IsNullOrEmpty(part) || part is "." or ".."))
            throw new ArgumentException("Invalid Skill resource path.", nameof(value));
        return path;
    }

    private static string Hash(string value) => Convert.ToHexString(
        SHA256.HashData(Encoding.UTF8.GetBytes(value))).ToLowerInvariant();

    private const string Router = """
        # 需求调研
        需求调研用于沉淀会影响项目范围、方案、风险、时间或验收的 Human 决策。信息已明确或只是临时沟通时不要创建调研。

        一次只激活当前目标对应的专业 Skill：
        - 缺少关键 Human 决策：SKreq-create；
        - 读取答案、备注、历史决定或既有方案：SKreq-read；
        - Human 已提交，需要形成正式方案：SKreq-resolve；
        - 已有方案，需要核对真实执行状态：SKreq-review。

        目标变化时再激活另一个 Skill。项目由程序绑定，不询问项目、Team 或 Room ID。
        """;

    private const string CreateSurvey = """
        # 创建需求调研
        1. 区分已确认事实、尚未确认的决定及其对范围、方案、风险、时间或验收的影响。
        2. requirement_survey_list(status=pending) 查重；对相近候选逐一 requirement_survey_get。
        3. 一张单只处理一个主题；设计 1–12 个单选/多选问题，每题 2–12 个具体、平行、可执行的选项，不创建自由文本题。
        4. 使用稳定的 request/question/option key 调用 requirement_survey_create。
        5. 验证 project_bound=true 和 pending；结果不明确时只能用相同 request_key 与相同内容重试。创建成功即结束，不代替 Human 提交。
        参数示例在 references/example.md。
        """;

    private const string ReadResults = """
        # 读取需求调研结果
        1. 刚提交的调研用 requirement_survey_list(status=submitted)，回顾历史可不传 status。
        2. 根据标题、purpose 和任务边界选候选，再 requirement_survey_get。
        3. 分别读取问题选择、notes 和 resolution；pending 不推断答案，submitted 且无 resolution 表示正式方案未形成。
        4. 分开输出 Human 决策、备注、Agent 方案、执行步骤及仍未确认事项。聊天摘要不能替代原始记录。
        输出示例在 references/example.md。
        """;

    private const string ResolveSurvey = """
        # 生成解决方案与执行计划
        1. requirement_survey_list(status=submitted) 后立即 get；只有 submitted 才继续。
        2. 把选择和备注映射到范围、方案、风险与验收；冲突时停止写入并列为待确认。
        3. summary 写最终决定和边界；solution_markdown 覆盖依据、范围、非目标、设计、迁移/兼容与验证。
        4. execution_steps 按依赖顺序写目标、动作、负责人建议、交付物和验收条件，不能宣称计划已经执行。
        5. resolve 后再次 get 回读验证。参数示例在 references/example.md。
        """;

    private const string ReviewExecution = """
        # 核对方案执行进度
        1. list/get 定位有 resolution 的基准方案；没有方案时退出。
        2. requirement_survey_project_tasks 读取项目任务事实。
        3. 按目标、范围、交付物和验收条件匹配步骤与 Todo，不只按标题。
        4. 只有 Completed 且 Result 提供验收证据才算完成；InProgress 是执行中；Blocked 带出原因；无对应 Todo 为未覆盖。
        5. 输出总体进度、逐步证据、阻塞、未覆盖和下一任务建议，不修改状态。示例在 references/example.md。
        """;

    private const string CreateExample = """
        {"request_key":"checkout-migration-2026-09","title":"结算模块迁移策略确认","purpose":"确认上线方式和旧接口保留周期。","questions":[{"key":"release_strategy","prompt":"采用哪种上线方式？","kind":"single_choice","required":true,"options":[{"key":"gradual","label":"按租户灰度上线"},{"key":"full","label":"一次性全量切换"}]}]}
        """;
    private const string ReadExample = """
        Human 已确认采用按租户灰度上线。备注要求首批仅内部租户；调研已 submitted，但 resolution 为空，尚未形成正式技术方案。
        """;
    private const string ResolveExample = """
        {"survey_id":"<来自 get>","summary":"采用灰度上线并保留回退能力。","solution_markdown":"## 实施范围\n实现灰度名单和回退开关。","execution_steps":[{"key":"design","title":"设计状态机","detail":"定义路由和回退条件。","owner":"架构负责人","deliverable":"设计说明","acceptance_criteria":"覆盖正常和回退路径"}]}
        """;
    private const string ReviewExample = """
        步骤“设计状态机”：已完成。证据：对应 Todo 为 Completed，Result 包含状态转换表。未覆盖：变更审计没有对应任务。阻塞：无。
        """;
}
