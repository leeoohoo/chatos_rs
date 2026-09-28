using ChatOS.Core.Domain;

namespace ChatOS.Core.Tests;

public sealed class StoryPromptCatalogTests
{
    [Fact]
    public void DefinitionsExposeUniqueStableKeys()
    {
        Assert.Equal(9, StoryPromptCatalog.Definitions.Count);
        Assert.Equal(
            StoryPromptCatalog.Definitions.Count,
            StoryPromptCatalog.Definitions.Select(definition => definition.Key).Distinct().Count());
        Assert.All(StoryPromptCatalog.Definitions, definition =>
        {
            Assert.StartsWith("story.", definition.Key);
            Assert.False(string.IsNullOrWhiteSpace(definition.Template));
        });
    }

    [Fact]
    public void RuntimeRenderersUseTheAuditedPromptContents()
    {
        var request = new StoryPlanningRequest(
            "text-model", "标题", "描述", "原文", "电影感", "16:9", 80);

        var planning = StoryPromptCatalog.RenderPlanningUser(request);
        var resource = StoryPromptCatalog.RenderResourceImage(
            "电影感", "红色风衣", "角色", "阿青", "16:9");
        var lastFrame = StoryPromptCatalog.RenderFrame(
            "电影感", "走到门前", true, "16:9", "上一段停在门外");
        var optimization = StoryPromptCatalog.RenderOptimizationUser(new StoryOptimizationRequest(
            "text-model", "标题", "描述", "完整原文", "电影感", StoryOptimizationTarget.Source));
        var refinement = StoryPromptCatalog.RenderSegmentRefinementUser(new StorySegmentRefinementRequest(
            "text-model", "标题", "摘要", "电影感", "16:9", "segment-1", "transition",
            "转场", "时间流逝", 3, "当前画面", "当前动作", "上一段在车站", "场景 station"));

        Assert.Contains("最多分段：80", planning);
        Assert.Contains("剧情原文：\n原文", planning);
        Assert.Contains("角色“阿青”", resource);
        Assert.Contains("红色风衣", resource);
        Assert.Contains("尾帧", lastFrame);
        Assert.Contains("上一段停在门外", lastFrame);
        Assert.Contains("优化目标：剧情原文", optimization);
        Assert.Contains("完整原文", optimization);
        Assert.Contains("类型：transition", refinement);
        Assert.Contains("上一段在车站", refinement);
        Assert.Equal("镜头推进", StoryPromptCatalog.RenderVideo("  镜头推进  "));
    }
}
