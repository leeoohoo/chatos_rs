using System.Collections.ObjectModel;
using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Presentation.Chat;

public sealed class ConversationTurnItemViewModel
{
    public ConversationTurnItemViewModel(LocalAgentMainChatTurn turn)
    {
        Id = turn.Binding.TurnId;
        RunId = turn.Run.RunId;
        RunVersion = turn.Run.Version;
        Revision = turn.Run.Version > long.MaxValue ? long.MaxValue : (long)turn.Run.Version;
        UserText = turn.Binding.UserMessage.Content ?? string.Empty;
        UserCreatedAt = turn.Binding.UserMessage.CreatedAt;
        Status = turn.Run.Status.ToString().ToLowerInvariant();
        IsRunning = !IsTerminal(turn.Run.Status);
        IsTaskGraphAvailable = turn.Tasks.Count > 0;
        Attachments = new ObservableCollection<ConversationAttachmentReference>(AttachmentsFrom(turn));
        ProcessEvents = new ObservableCollection<TurnProcessItemViewModel>(
            turn.Detail.Events
                .Where(static value => value.EventType is not
                    ("message_user_content" or "message_assistant_content"))
                .Select(value => new TurnProcessItemViewModel(
                    value.EventId,
                    DisplayEventType(value.EventType),
                    value.Message,
                    Status)));
        var replies = new List<ConversationReplyItemViewModel>();
        var assistant = turn.Detail.Events.LastOrDefault(static value =>
            value.EventType == "message_assistant_content" && !string.IsNullOrWhiteSpace(value.Message));
        if (assistant is not null)
        {
            replies.Add(new ConversationReplyItemViewModel(
                assistant.EventId,
                assistant.Message!,
                assistant.CreatedAt,
                null,
                null,
                null,
                null));
        }
        replies.AddRange(turn.Tasks.Select(task => new ConversationReplyItemViewModel(
            $"task-{task.TaskId}",
            task.Objective,
            task.UpdatedAt,
            task.TaskId,
            task.CurrentRunId,
            task.Status,
            new MessageTaskGraphRequest(
                task.SourceThreadId,
                task.SourceTurnId,
                task.TaskId,
                task.CurrentRunId))));
        Replies = new ObservableCollection<ConversationReplyItemViewModel>(replies);
    }

    public string Id { get; }

    public string RunId { get; }

    public ulong RunVersion { get; }

    public long Revision { get; }

    public string UserText { get; }

    public DateTimeOffset UserCreatedAt { get; }

    public string Status { get; }

    public bool IsRunning { get; }

    public bool IsTaskGraphAvailable { get; }

    public ObservableCollection<ConversationAttachmentReference> Attachments { get; }

    public ObservableCollection<TurnProcessItemViewModel> ProcessEvents { get; }

    public ObservableCollection<ConversationReplyItemViewModel> Replies { get; }

    private static bool IsTerminal(LocalAgentRunStatus status) => status is
        LocalAgentRunStatus.Succeeded or LocalAgentRunStatus.Failed or LocalAgentRunStatus.Cancelled;

    private static string DisplayEventType(string value) =>
        string.Join(' ', value.Split('_', StringSplitOptions.RemoveEmptyEntries)
            .Select(word => char.ToUpperInvariant(word[0]) + word[1..]));

    private static IReadOnlyList<ConversationAttachmentReference> AttachmentsFrom(
        LocalAgentMainChatTurn turn)
    {
        var payload = turn.Binding.UserMessage.StructuredPayload;
        if (payload is not { ValueKind: System.Text.Json.JsonValueKind.Object }
            || !payload.Value.TryGetProperty("attachments", out var attachments)
            || attachments.ValueKind != System.Text.Json.JsonValueKind.Array)
        {
            return [];
        }
        var values = new List<ConversationAttachmentReference>();
        foreach (var item in attachments.EnumerateArray())
        {
            if (!item.TryGetProperty("attachment_id", out var idValue)
                || idValue.GetString() is not { Length: > 0 } id
                || !item.TryGetProperty("media_type", out var mediaValue)
                || mediaValue.GetString() is not { Length: > 0 } mediaType
                || !item.TryGetProperty("byte_size", out var sizeValue)
                || !sizeValue.TryGetInt32(out var size))
            {
                continue;
            }
            var kind = mediaType.StartsWith("image/", StringComparison.OrdinalIgnoreCase)
                ? ConversationAttachmentKind.Image
                : mediaType.StartsWith("audio/", StringComparison.OrdinalIgnoreCase)
                    ? ConversationAttachmentKind.Audio
                    : ConversationAttachmentKind.File;
            values.Add(new ConversationAttachmentReference(id, id, mediaType, size, kind));
        }
        return values;
    }
}

public sealed record TurnProcessItemViewModel(
    string Id,
    string Title,
    string? Detail,
    string Status);

public sealed record ConversationReplyItemViewModel(
    string Id,
    string Text,
    DateTimeOffset CreatedAt,
    string? TaskId,
    string? RunId,
    string? TaskStatus,
    MessageTaskGraphRequest? TaskGraphRequest)
{
    public bool IsTaskCallback => !string.IsNullOrWhiteSpace(TaskId);
}

public sealed record MessageTaskGraphRequest(
    string SourceThreadId,
    string SourceTurnId,
    string TaskId,
    string? RunId);

public sealed partial class AskUserFieldInputViewModel : ObservableObject
{
    public AskUserFieldInputViewModel(AskUserField field)
    {
        Key = field.Key;
        Label = field.Label;
        Description = field.Description;
        Placeholder = field.Placeholder;
        IsRequired = field.IsRequired;
        IsMultiline = field.IsMultiline;
        IsSecret = field.IsSecret;
        Value = field.DefaultValue;
    }

    public string Key { get; }

    public string Label { get; }

    public string? Description { get; }

    public string? Placeholder { get; }

    public bool IsRequired { get; }

    public bool IsMultiline { get; }

    public bool IsSecret { get; }

    [ObservableProperty]
    private string _value;
}

public sealed partial class AskUserChoiceOptionViewModel : ObservableObject
{
    public AskUserChoiceOptionViewModel(AskUserChoiceOption option, bool selected)
    {
        Value = option.Value;
        Label = option.Label;
        Description = option.Description;
        IsSelected = selected;
    }

    public string Value { get; }

    public string Label { get; }

    public string? Description { get; }

    [ObservableProperty]
    private bool _isSelected;
}
