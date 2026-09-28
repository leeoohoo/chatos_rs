using System.Collections.ObjectModel;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed class StoryContinuityIssue
{
    public StoryContinuityIssue(
        StorySegmentEditor segment,
        string severity,
        string title,
        string detail)
    {
        Segment = segment;
        Severity = severity;
        Title = title;
        Detail = detail;
    }

    public StorySegmentEditor Segment { get; }
    public string Severity { get; }
    public string Title { get; }
    public string Detail { get; }
    public string SegmentLabel => $"{Segment.NumberLabel} · {Segment.Title}";
}

public sealed partial class StoryStudioViewModel
{
    public ObservableCollection<StoryContinuityIssue> ContinuityIssues { get; } = [];

    public bool CanAutoFillContinuity => !IsBusy && _current is not null && Segments.Count > 0;
    public string ContinuitySummary => ContinuityIssues.Count == 0
        ? "连续性检查 · 已就绪"
        : $"连续性检查 · {ContinuityIssues.Count} 项待处理";

    public void SelectContinuityIssue(StoryContinuityIssue issue)
    {
        if (!IsBusy && Segments.Contains(issue.Segment)) SelectedSegment = issue.Segment;
    }

    public async Task AutoFillContinuityAsync(CancellationToken cancellationToken = default)
    {
        if (!CanAutoFillContinuity) return;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            FillMissingContinuity();
            await PersistCurrentAsync(cancellationToken);
            StatusMessage = "已补齐缺失的连续性说明和基础镜头计划";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
        finally
        {
            IsBusy = false;
            RefreshContinuityAudit();
        }
    }

    private void FillMissingContinuity()
    {
        for (var index = 0; index < Segments.Count; index++)
        {
            var segment = Segments[index];
            if (string.IsNullOrWhiteSpace(segment.Narrative)) continue;
            if (string.IsNullOrWhiteSpace(segment.ContinuityIn))
            {
                segment.ContinuityIn = index == 0
                    ? $"故事开场：{Clip(segment.Narrative, 600)}"
                    : $"承接上一段“{Segments[index - 1].Title}”的结尾状态：{Clip(PreviousEnding(index), 900)}";
            }
            if (string.IsNullOrWhiteSpace(segment.ContinuityOut))
            {
                segment.ContinuityOut = index + 1 == Segments.Count
                    ? $"本段收束全剧：{Clip(segment.Narrative, 600)}"
                    : $"为下一段“{Segments[index + 1].Title}”保留衔接：{Clip(Segments[index + 1].Narrative, 900)}";
            }
            if (string.IsNullOrWhiteSpace(segment.ShotPlan))
            {
                var action = string.IsNullOrWhiteSpace(segment.VideoPrompt)
                    ? segment.Narrative
                    : segment.VideoPrompt;
                segment.ShotPlan = $"0–{segment.Seconds} 秒：{Clip(action, 1_800)}";
            }
        }
    }

    private string BuildContinuityContext(StorySegmentEditor segment)
    {
        var index = Segments.IndexOf(segment);
        if (index < 0) return string.Empty;
        var lines = new List<string>();
        lines.Add($"本段类型：{segment.KindLabel}");
        if (!string.IsNullOrWhiteSpace(ProjectSummary))
            lines.Add($"全剧摘要：{Clip(ProjectSummary, 700)}");
        if (index > 0)
        {
            var previous = Segments[index - 1];
            lines.Add($"上一段：{previous.Title}；结尾状态：{Clip(PreviousEnding(index), 900)}");
        }
        lines.Add($"本段衔接起点：{Clip(segment.ContinuityIn, 900)}");
        lines.Add($"本段镜头计划：{Clip(segment.ShotPlan, 1_800)}");
        lines.Add($"本段衔接终点：{Clip(segment.ContinuityOut, 900)}");
        if (index + 1 < Segments.Count)
        {
            var next = Segments[index + 1];
            lines.Add($"下一段：{next.Title}；开场需求：{Clip(next.ContinuityIn, 900)}");
        }
        var resourceIds = segment.ToDocument().ResourceIds.ToHashSet(StringComparer.Ordinal);
        var names = Resources
            .Where(resource => resourceIds.Contains(resource.Id))
            .Select(resource => $"{resource.KindLabel}“{resource.Name}”");
        var resourceLabel = string.Join("、", names);
        if (resourceLabel.Length > 0) lines.Add($"本段一致性素材：{resourceLabel}");
        return string.Join("\n", lines.Where(line => !line.EndsWith("：", StringComparison.Ordinal)));
    }

    private string PreviousEnding(int currentIndex)
    {
        var previous = Segments[currentIndex - 1];
        return string.IsNullOrWhiteSpace(previous.ContinuityOut)
            ? previous.Narrative
            : previous.ContinuityOut;
    }

    private void RefreshContinuityAudit()
    {
        ContinuityIssues.Clear();
        if (_current is null)
        {
            NotifyContinuityChanged();
            return;
        }
        foreach (var segment in Segments)
        {
            AddMissingIssue(segment, segment.ContinuityIn, "缺少衔接起点", "生成画面和视频前应说明本段从什么人物、动作和画面状态开始。");
            AddMissingIssue(segment, segment.ContinuityOut, "缺少衔接终点", "应说明本段结束时需要留给下一段的状态。");
            AddMissingIssue(segment, segment.ShotPlan, "缺少镜头计划", "至少需要一条覆盖本段时长的动作或镜头说明。");
            if (segment.Seconds > 15)
            {
                ContinuityIssues.Add(new StoryContinuityIssue(
                    segment, "时长", "分段超过 15 秒", "建议拆分分段，避免视频模型自动缩短后破坏节奏。"));
            }
            var ids = segment.ToDocument().ResourceIds.ToHashSet(StringComparer.Ordinal);
            foreach (var resource in Resources.Where(resource => ids.Contains(resource.Id) && string.IsNullOrWhiteSpace(resource.ImagePath)))
            {
                ContinuityIssues.Add(new StoryContinuityIssue(
                    segment, "素材", $"{resource.Name} 缺少参考图", "先生成或导入一致性参考图，再制作本段首尾帧。"));
            }
        }
        NotifyContinuityChanged();
    }

    private void AddMissingIssue(
        StorySegmentEditor segment,
        string value,
        string title,
        string detail)
    {
        if (string.IsNullOrWhiteSpace(value))
            ContinuityIssues.Add(new StoryContinuityIssue(segment, "衔接", title, detail));
    }

    private void NotifyContinuityChanged()
    {
        OnPropertyChanged(nameof(ContinuitySummary));
        OnPropertyChanged(nameof(CanAutoFillContinuity));
    }

    private static string Clip(string value, int maximum) => string.IsNullOrWhiteSpace(value)
        ? string.Empty
        : value.Trim()[..Math.Min(value.Trim().Length, maximum)];

    partial void OnProjectSummaryChanged(string value)
    {
        RefreshPromptAudit();
        RefreshContinuityAudit();
        NotifySegmentRefinementChanged();
    }
}
