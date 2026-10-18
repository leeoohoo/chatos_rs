using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Chat;

public sealed partial class ConversationSessionViewModel
{
    private void ResetVisualState(
        string? conversationId,
        string title,
        bool isPluginInstaller)
    {
        var preservePendingRetry = string.Equals(
            ConversationId, conversationId, StringComparison.Ordinal) &&
            !string.IsNullOrWhiteSpace(conversationId);
        ResetHistoryRetry();
        ConversationId = conversationId;
        Title = title;
        IsPluginInstaller = isPluginInstaller;
        IsOpen = !string.IsNullOrWhiteSpace(conversationId);
        IsLoading = IsOpen;
        IsLoadingOlder = false;
        IsSending = false;
        IsRunning = false;
        HasOlder = false;
        UnreadNewerCount = 0;
        ErrorMessage = null;
        Draft = string.Empty;
        CloseReplyInspectors();
        Turns.Clear();
        LiveProcesses.Clear();
        PendingPrompts.Clear();
        _resolvedPromptIds.Clear();
        Attachments.Clear();
        if (!preservePendingRetry) _pendingNewTurnRetry = null;
        Models.Clear();
        ReasoningLevels.Clear();
        OnPropertyChanged(nameof(HasReasoningLevels));
        SelectedModel = null;
        SelectedThinkingLevel = null;
        RemoteConnections.Clear();
        SelectedRemoteConnection = null;
        Capabilities.Clear();
        SelectedCapability = null;
        ReasoningEnabled = false;
        AttachmentError = null;
    }

    private void RestoreAttachments(IEnumerable<ConversationAttachmentDraft> attachments)
    {
        var existingIds = Attachments.Select(static value => value.Id).ToHashSet(StringComparer.Ordinal);
        var insertIndex = 0;
        foreach (var attachment in attachments)
        {
            if (existingIds.Add(attachment.Id))
            {
                Attachments.Insert(insertIndex++, attachment);
            }
        }
    }

    private static ConversationAttachmentReference ToReference(ConversationAttachmentDraft value) => new(
        value.Id,
        value.Name,
        value.MimeType,
        value.Size,
        value.Kind);

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

    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;

    private void OnLocalizationChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        OnPropertyChanged(nameof(AttachmentTotalSizeLabel));
        OnPropertyChanged(nameof(UnreadNewerLabel));
        OnPropertyChanged(nameof(CapabilityPickerLabel));
        OnPropertyChanged(nameof(PluginInstallerBoundaryText));
    }

    private void CancelCurrentSession()
    {
        CloseReplyInspectors();
        Interlocked.Increment(ref _promptRefreshGeneration);
        foreach (var prompt in PendingPrompts) prompt.Deactivate();
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = null;
        _realtimeTask = null;
        ResetHistoryRetry();
    }

    private void CloseReplyInspectors()
    {
        foreach (var turn in Turns)
            foreach (var reply in turn.Replies)
                reply.CloseInspector();
    }

    private void ResetHistoryRetry()
    {
        _historyRetryCancellation?.Cancel();
        _historyRetryCancellation?.Dispose();
        _historyRetryCancellation = null;
        _historyRetryAttempt = 0;
        _historyRetryTask = null;
    }
}
