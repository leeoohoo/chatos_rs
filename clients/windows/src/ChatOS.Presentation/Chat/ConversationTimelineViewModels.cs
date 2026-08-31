using System.Collections.ObjectModel;
using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Presentation.Chat;

public sealed class ConversationTurnItemViewModel
{
    public ConversationTurnItemViewModel(ConversationTurn turn)
    {
        Id = turn.Id;
        Revision = turn.Revision;
        UserText = turn.UserMessage.Text;
        UserCreatedAt = turn.UserMessage.CreatedAt;
        Status = turn.Status.ToString().ToLowerInvariant();
        IsRunning = turn.Status == TurnStatus.Streaming;
        MessageTaskLookup = turn.MessageTaskLookup;
        IsTaskGraphAvailable = turn.IsTaskGraphAvailable;
        TaskGraphMessageId = turn.MessageTaskLookup?.SourceUserMessageId ?? turn.UserMessage.Id;
        Attachments = new ObservableCollection<ConversationAttachmentReference>(turn.UserMessage.Attachments);
        ProcessEvents = new ObservableCollection<TurnProcessItemViewModel>(
            turn.ProcessEvents.Select(static value => new TurnProcessItemViewModel(
                value.Id,
                value.Title,
                value.Detail,
                value.Status.ToString().ToLowerInvariant())));
        Replies = new ObservableCollection<ConversationReplyItemViewModel>(
            turn.AssistantReplies.Count > 0
                ? turn.AssistantReplies.Select(reply => new ConversationReplyItemViewModel(
                    reply.Message.Id,
                    reply.Message.Text,
                    reply.Message.CreatedAt,
                    reply.TaskCallback?.TaskId,
                    reply.TaskCallback?.RunId,
                    reply.TaskCallback?.Status,
                    reply.TaskCallback is null
                        ? null
                        : new MessageTaskGraphRequest(
                            TaskGraphMessageId,
                            reply.TaskCallback.TaskId,
                            reply.TaskCallback.RunId,
                            turn.MessageTaskLookup ?? new MessageTaskLookup(
                                turn.ConversationId,
                                turn.Id,
                                turn.UserMessage.Id))))
                : turn.FinalAssistantMessage is { } final
                    ? new[]
                    {
                        new ConversationReplyItemViewModel(
                            final.Id,
                            final.Text,
                            final.CreatedAt,
                            null,
                            null,
                            null,
                            null),
                    }
                    : Array.Empty<ConversationReplyItemViewModel>());
    }

    public string Id { get; }

    public long Revision { get; }

    public string UserText { get; }

    public DateTimeOffset UserCreatedAt { get; }

    public string Status { get; }

    public bool IsRunning { get; }

    public MessageTaskLookup? MessageTaskLookup { get; }

    public bool IsTaskGraphAvailable { get; }

    public string TaskGraphMessageId { get; }

    public ObservableCollection<ConversationAttachmentReference> Attachments { get; }

    public ObservableCollection<TurnProcessItemViewModel> ProcessEvents { get; }

    public ObservableCollection<ConversationReplyItemViewModel> Replies { get; }
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
    string MessageId,
    string TaskId,
    string? RunId,
    MessageTaskLookup Lookup);

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
