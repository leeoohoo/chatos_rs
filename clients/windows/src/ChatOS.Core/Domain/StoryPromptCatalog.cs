namespace ChatOS.Core.Domain;

public sealed record StoryPromptDefinition(
    string Key,
    string Name,
    string Purpose,
    string Template);

public static class StoryPromptCatalog
{
    public const string PlanningSystemKey = "story.plan.system";
    public const string PlanningUserKey = "story.plan.user";
    public const string ResourceImageKey = "story.resource.image";
    public const string FirstFrameKey = "story.segment.first-frame";
    public const string LastFrameKey = "story.segment.last-frame";
    public const string VideoKey = "story.segment.video";

    public const string PlanningSystem =
        "你是影视剧情规划师。只返回满足 JSON Schema 的计划。先建立可复用的角色、场景、道具表，再按原文顺序连续覆盖故事，不添加原文没有的事实。每段必须引用实际使用的素材 ID，并能独立制作成 2-15 秒视频；图片提示词描述静态画面，视频提示词描述动作、镜头和节奏。";

    public static IReadOnlyList<StoryPromptDefinition> Definitions { get; } =
    [
        new(
            PlanningSystemKey,
            "全剧规划 · 系统约束",
            "约束文本模型输出可验证的素材表和连续分段。",
            PlanningSystem),
        new(
            PlanningUserKey,
            "全剧规划 · 项目内容",
            "把当前项目资料和剧情原文提交给所选文本模型。",
            "标题：{title}\n描述：{description}\n画面风格：{visual_style}\n比例：{ratio}\n最多分段：{maximum_segments}\n\n剧情原文：\n{source}"),
        new(
            ResourceImageKey,
            "素材一致性参考图",
            "为角色、场景或道具生成后续分段可复用的参考图。",
            "{visual_style}\n{resource_image_prompt}\n生成{resource_kind}“{resource_name}”的一致性参考图，画面比例 {ratio}。"),
        new(
            FirstFrameKey,
            "分段首帧",
            "结合分段所关联的素材参考图生成首帧。",
            "{visual_style}\n{segment_image_prompt}\n生成该分段的首帧，画面比例 {ratio}。"),
        new(
            LastFrameKey,
            "分段尾帧",
            "结合首帧和分段所关联的素材参考图生成尾帧。",
            "{visual_style}\n{segment_image_prompt}\n生成该分段的尾帧，画面比例 {ratio}。"),
        new(
            VideoKey,
            "分段视频",
            "把分段视频提示词及已有首尾帧提交给所选视频模型。",
            "{segment_video_prompt}"),
    ];

    public static string RenderPlanningUser(StoryPlanningRequest request) =>
        $"标题：{request.Title}\n描述：{request.Description}\n画面风格：{request.VisualStyle}\n比例：{request.Ratio}\n最多分段：{request.MaximumSegments}\n\n剧情原文：\n{request.Source}";

    public static string RenderResourceImage(
        string visualStyle,
        string imagePrompt,
        string kindLabel,
        string name,
        string ratio) =>
        $"{visualStyle}\n{imagePrompt.Trim()}\n生成{kindLabel}“{name}”的一致性参考图，画面比例 {ratio}。";

    public static string RenderFrame(
        string visualStyle,
        string imagePrompt,
        bool lastFrame,
        string ratio) =>
        $"{visualStyle}\n{imagePrompt.Trim()}\n生成该分段的{(lastFrame ? "尾帧" : "首帧")}，画面比例 {ratio}。";

    public static string RenderVideo(string videoPrompt) => videoPrompt.Trim();
}
