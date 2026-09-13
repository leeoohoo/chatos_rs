using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsLocalAgentRunSource(
    string ThreadId,
    string TurnId,
    LocalAgentTaskSnapshot? Task);

/// <summary>
/// The single Windows mapping from a durable Run to its immutable Main Chat
/// or current Task source. Interaction services must not infer source identity
/// independently from UI selection.
/// </summary>
internal static class WindowsLocalAgentRunSourceResolver
{
    public static WindowsLocalAgentRunSource? Resolve(
        WindowsLocalAgentProjectionSnapshot projection,
        WindowsLocalAgentRecoveredRun recovered)
    {
        var run = recovered.Run;
        if (!string.Equals(run.OwnerUserId, projection.AccountId, StringComparison.Ordinal))
            throw new InvalidDataException("The Local Agent run belongs to another account.");
        if (run.ProfileKey == "main_chat")
        {
            var binding = recovered.MainChatBinding
                ?? throw new InvalidDataException("The Main Chat run has no source binding.");
            WindowsLocalAgentStartupRecovery.ValidateBinding(run, binding);
            return new WindowsLocalAgentRunSource(binding.ThreadId, binding.TurnId, null);
        }
        if (run.ProfileKey != "task_runner") return null;
        var tasks = projection.Tasks.Values.Where(task =>
                task.RunIds.Contains(run.RunId, StringComparer.Ordinal))
            .ToArray();
        if (tasks.Length != 1)
            throw new InvalidDataException("The Task run has no unique source binding.");
        var task = tasks[0];
        if (!string.Equals(task.TaskId, run.OwnerEntityId, StringComparison.Ordinal)
            || !string.Equals(task.ProjectId, run.ProjectId, StringComparison.Ordinal))
            throw new InvalidDataException("The Task run changed its frozen source identity.");
        if (!string.Equals(task.CurrentRunId, run.RunId, StringComparison.Ordinal)) return null;
        return new WindowsLocalAgentRunSource(task.SourceThreadId, task.SourceTurnId, task);
    }
}
