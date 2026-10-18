using System.Collections.ObjectModel;
using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Chat;

public sealed partial class ConversationTurnItemViewModel : ObservableObject
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
        Attachments = new ObservableCollection<ConversationAttachmentItemViewModel>(
            turn.UserMessage.Attachments.Select(static attachment =>
                new ConversationAttachmentItemViewModel(attachment)));
        ProcessEvents = new ObservableCollection<TurnProcessItemViewModel>(
            turn.ProcessEvents.Select(static value => new TurnProcessItemViewModel(
                value.Id,
                value.Title,
                PresentProviderError(value.Detail),
                value.Status.ToString().ToLowerInvariant())));
        Replies = new ObservableCollection<ConversationReplyItemViewModel>(
            turn.AssistantReplies.Count > 0
                ? turn.AssistantReplies.Select(reply => new ConversationReplyItemViewModel(
                    reply.Message.Id,
                    PresentProviderError(reply.Message.Text) ?? string.Empty,
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
                            PresentProviderError(final.Text) ?? string.Empty,
                            final.CreatedAt,
                            null,
                            null,
                            null,
                            null),
                    }
                    : Array.Empty<ConversationReplyItemViewModel>());
        TimelineMessages = BuildTimelineMessages(turn);
        TaskGraphRequest = Replies
            .Select(static reply => reply.TaskGraphRequest)
            .FirstOrDefault(static request => request is not null)
            ?? (turn.MessageTaskLookup is { } lookup
                ? new MessageTaskGraphRequest(
                    TaskGraphMessageId,
                    string.Empty,
                    null,
                    lookup)
                : null);
    }

    public string Id { get; }

    public string AutomationId => $"ChatOS.Chat.Turn.{Id}";

    public string UserMessageAutomationId => $"ChatOS.Chat.UserMessage.{Id}";

    public string ProcessToggleAutomationId => $"ChatOS.Chat.ProcessToggle.{Id}";

    public string TaskGraphAutomationId => $"ChatOS.Chat.TaskGraph.Open.{Id}";

    public string AutomationStatus => $"turn={Id}; revision={Revision}; status={Status}";

    public long Revision { get; }

    public string UserText { get; }

    public DateTimeOffset UserCreatedAt { get; }

    public string UserTimeLabel => UserCreatedAt.ToLocalTime().ToString("HH:mm");

    public string Status { get; }

    public bool IsRunning { get; }

    public MessageTaskLookup? MessageTaskLookup { get; }

    public bool IsTaskGraphAvailable { get; }

    public bool HasProcessEvents => ProcessEvents.Count > 0;

    public string TaskGraphMessageId { get; }

    public ObservableCollection<ConversationAttachmentItemViewModel> Attachments { get; }

    public ObservableCollection<TurnProcessItemViewModel> ProcessEvents { get; }

    public ObservableCollection<ConversationReplyItemViewModel> Replies { get; }

    public IReadOnlyList<object> TimelineMessages { get; }

    public MessageTaskGraphRequest? TaskGraphRequest { get; }

    private IReadOnlyList<object> BuildTimelineMessages(ConversationTurn turn)
    {
        var additionalUsers = (turn.AdditionalUserMessages ?? [])
            .OrderBy(static message => message, Comparer<ChatMessage>.Create(static (left, right) =>
            {
                if (left.StorageOrdinal is { } leftOrdinal &&
                    right.StorageOrdinal is { } rightOrdinal && leftOrdinal != rightOrdinal)
                {
                    return leftOrdinal.CompareTo(rightOrdinal);
                }

                return left.CreatedAt.CompareTo(right.CreatedAt);
            })).ToArray();
        IReadOnlyList<ChatMessage> replies = turn.AssistantReplies.Count > 0
            ? turn.AssistantReplies.Select(static reply => reply.Message).ToArray()
            : turn.FinalAssistantMessage is { } final ? [final] : [];
        var items = new List<object>(additionalUsers.Length + Replies.Count);
        var nextUser = 0;
        for (var index = 0; index < replies.Count; index++)
        {
            var reply = replies[index];
            // Preserve the handoff/callback presentation order. Only guidance is
            // inserted using durable ordinals, with timestamps for older history.
            while (nextUser < additionalUsers.Length && PrecedesReply(additionalUsers[nextUser], reply))
            {
                items.Add(new ConversationUserMessageItemViewModel(additionalUsers[nextUser++]));
            }

            items.Add(Replies[index]);
        }

        while (nextUser < additionalUsers.Length)
        {
            items.Add(new ConversationUserMessageItemViewModel(additionalUsers[nextUser++]));
        }

        return items;
    }

    private static bool PrecedesReply(ChatMessage user, ChatMessage reply) =>
        user.StorageOrdinal is { } userOrdinal && reply.StorageOrdinal is { } replyOrdinal
            ? userOrdinal <= replyOrdinal
            : user.CreatedAt <= reply.CreatedAt;

    private static string? PresentProviderError(string? value)
    {
        if (string.IsNullOrWhiteSpace(value) ||
            !value.Contains("403 Forbidden", StringComparison.OrdinalIgnoreCase) ||
            !value.Contains("vip", StringComparison.OrdinalIgnoreCase))
        {
            return value;
        }

        return "当前账号没有权限使用所选 AI 模型（vip 分组）。请在设置 → AI 模型配置中选择有权限的模型，或联系管理员开通权限。";
    }

    [ObservableProperty]
    private bool _isProcessExpanded;

    [RelayCommand]
    private void ToggleProcess() => IsProcessExpanded = !IsProcessExpanded;
}

public sealed class ConversationUserMessageItemViewModel
{
    public ConversationUserMessageItemViewModel(ChatMessage message)
    {
        Id = message.Id;
        Text = message.Text;
        CreatedAt = message.CreatedAt;
        Attachments = message.Attachments.Select(static attachment =>
            new ConversationAttachmentItemViewModel(attachment)).ToArray();
    }

    public string Id { get; }

    public string GuidanceAutomationId => $"ChatOS.Chat.GuidanceMessage.{Id}";

    public string Text { get; }

    public DateTimeOffset CreatedAt { get; }

    public string TimeLabel => CreatedAt.ToLocalTime().ToString("HH:mm");

    public IReadOnlyList<ConversationAttachmentItemViewModel> Attachments { get; }
}

public sealed class ConversationAttachmentItemViewModel
{
    public ConversationAttachmentItemViewModel(ConversationAttachmentReference attachment)
    {
        Attachment = attachment;
    }

    public ConversationAttachmentReference Attachment { get; }

    public string Name => Attachment.Name;

    public string MimeType => Attachment.MimeType;

    public int Size => Attachment.Size;

    public string SizeLabel => FormatByteCount(Attachment.Size);

    public bool HasImagePreview =>
        Attachment.Kind == ConversationAttachmentKind.Image &&
        Uri.TryCreate(Attachment.ViewUrl ?? Attachment.Url, UriKind.Absolute, out _);

    public bool IsFileChip => !HasImagePreview;

    private static string FormatByteCount(long bytes)
    {
        if (bytes < 1024)
        {
            return $"{bytes} B";
        }

        if (bytes < 1024 * 1024)
        {
            return $"{bytes / 1024d:0.#} KB";
        }

        return $"{bytes / (1024d * 1024d):0.#} MB";
    }
}

public sealed record TurnProcessItemViewModel(
    string Id,
    string Title,
    string? Detail,
    string Status)
{
    public string LiveAutomationId => $"ChatOS.Chat.LiveProcess.{Id}";

    public string HistoryAutomationId => $"ChatOS.Chat.HistoryProcess.{Id}";
}

public enum TaskReplyInspectorSection
{
    None,
    Process,
    Detail,
}

public sealed partial class ConversationReplyItemViewModel : ObservableObject
{
    public ConversationReplyItemViewModel(
        string id,
        string text,
        DateTimeOffset createdAt,
        string? taskId,
        string? runId,
        string? taskStatus,
        MessageTaskGraphRequest? taskGraphRequest)
    {
        Id = id;
        Text = text;
        CreatedAt = createdAt;
        TaskId = taskId;
        RunId = runId;
        TaskStatus = taskStatus;
        TaskGraphRequest = taskGraphRequest;
    }

    public string Id { get; }

    public string AssistantAutomationId => $"ChatOS.Chat.AssistantMessage.{Id}";

    public string AutomationStatus => IsTaskCallback
        ? $"message={Id}; task={TaskId}; run={RunId ?? "none"}; status={TaskStatus ?? "unknown"}"
        : $"message={Id}; status=completed";

    public string Text { get; }

    public DateTimeOffset CreatedAt { get; }

    public string? TaskId { get; }

    public string? RunId { get; }

    public string? TaskStatus { get; }

    public MessageTaskGraphRequest? TaskGraphRequest { get; }

    public bool IsTaskCallback => !string.IsNullOrWhiteSpace(TaskId);

    public string TimeLabel => CreatedAt.ToLocalTime().ToString("HH:mm");

    public bool HasInspector => InspectorSection != TaskReplyInspectorSection.None;

    public bool IsProcessExpanded => InspectorSection == TaskReplyInspectorSection.Process;

    public bool IsDetailExpanded => InspectorSection == TaskReplyInspectorSection.Detail;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasInspector))]
    [NotifyPropertyChangedFor(nameof(IsProcessExpanded))]
    [NotifyPropertyChangedFor(nameof(IsDetailExpanded))]
    private TaskReplyInspectorSection _inspectorSection;

    [ObservableProperty]
    private bool _isInspectorLoading;

    [ObservableProperty]
    private MessageTask? _inspectorTask;

    [ObservableProperty]
    private MessageTaskRunDetail? _inspectorRun;

    [ObservableProperty]
    private string? _inspectorError;

    public bool BeginInspector(TaskReplyInspectorSection section)
    {
        if (section == TaskReplyInspectorSection.None || InspectorSection == section)
        {
            CloseInspector();
            return false;
        }

        CancelInspectorLoad();
        InspectorSection = section;
        IsInspectorLoading = true;
        InspectorError = null;
        return true;
    }

    private void CompleteInspector(MessageTask task, MessageTaskRunDetail? run)
    {
        InspectorTask = task;
        InspectorRun = run;
        InspectorError = null;
        IsInspectorLoading = false;
    }

    private void FailInspector(string message)
    {
        InspectorError = message;
        IsInspectorLoading = false;
    }
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

    public string TextAutomationId => $"ChatOS.Chat.AskUser.Field.{Key}";

    public string SecretAutomationId => $"ChatOS.Chat.AskUser.Secret.{Key}";

    public string PetTextAutomationId => $"ChatOS.Pet.AskUser.Field.{Key}";

    public string PetSecretAutomationId => $"ChatOS.Pet.AskUser.Secret.{Key}";

    public string Label { get; }

    public string? Description { get; }

    public string? Placeholder { get; }

    public bool IsRequired { get; }

    public bool IsMultiline { get; }

    public bool IsSecret { get; }

    public double InputMinHeight => IsMultiline ? 86 : 34;

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

    public string OptionAutomationId => $"ChatOS.Chat.AskUser.Option.{Value}";

    public string PetOptionAutomationId => $"ChatOS.Pet.AskUser.Option.{Value}";

    public string Label { get; }

    public string? Description { get; }

    [ObservableProperty]
    private bool _isSelected;
}
