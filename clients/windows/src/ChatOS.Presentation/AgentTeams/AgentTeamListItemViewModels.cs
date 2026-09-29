using ChatOS.Core.Domain;

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
}
