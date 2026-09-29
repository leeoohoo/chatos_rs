using ChatOS.Core.Domain;
using System.Globalization;

namespace ChatOS.Presentation.AgentTeams;

public sealed class AgentTodoItemViewModel(AgentTodo todo, string? agentName)
{
    public AgentTodo Todo { get; } = todo;
    public string Title => Todo.Draft.Title;
    public string Detail => Todo.Draft.Detail;
    public string AssigneeLabel { get; } = string.IsNullOrWhiteSpace(agentName)
        ? todo.Draft.AgentId
        : agentName;
    public string StatusLabel => AgentTeamDisplayText.For(Todo.Status);
    public string PriorityLabel => $"{AgentTeamDisplayText.For(Todo.Draft.Priority)}优先级";
}

public sealed class AgentRunItemViewModel(AgentRunSummary run, string? agentName)
{
    public AgentRunSummary Run { get; } = run;
    public string AgentLabel { get; } = string.IsNullOrWhiteSpace(agentName)
        ? run.AgentId
        : agentName;
    public string StatusLabel => AgentTeamDisplayText.For(Run.Status);
    public string ModelCallsLabel => $"模型调用 {Run.ModelCalls} 次";
    public string? LastError => Run.LastError;
    public string StartedAtLabel => FormatTimestamp(Run.CreatedAtUnixMs);
    public string UpdatedAtLabel => FormatTimestamp(Run.UpdatedAtUnixMs);
    public string DurationLabel => FormatDuration(Run.UpdatedAtUnixMs - Run.CreatedAtUnixMs);

    private static string FormatTimestamp(long unixMilliseconds) =>
        DateTimeOffset.FromUnixTimeMilliseconds(unixMilliseconds).ToLocalTime()
            .ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.CurrentCulture);

    private static string FormatDuration(long milliseconds)
    {
        var duration = TimeSpan.FromMilliseconds(Math.Max(0, milliseconds));
        if (duration.TotalHours >= 1)
            return $"{(int)duration.TotalHours} 小时 {duration.Minutes} 分";
        if (duration.TotalMinutes >= 1)
            return $"{(int)duration.TotalMinutes} 分 {duration.Seconds} 秒";
        return $"{duration.Seconds} 秒";
    }
}

public sealed class AgentRunFilterOption(string? agentId, string label)
{
    public string? AgentId { get; } = agentId;
    public string Label { get; } = label;
}
