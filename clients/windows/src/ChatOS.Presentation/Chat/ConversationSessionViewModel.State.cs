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
    private void ResetVisualState(string? conversationId, string title)
    {
        ConversationId = conversationId;
        Title = title;
        IsOpen = !string.IsNullOrWhiteSpace(conversationId);
        IsLoading = IsOpen;
        IsLoadingOlder = false;
        IsSending = false;
        IsRunning = false;
        HasOlder = false;
        UnreadNewerCount = 0;
        ErrorMessage = null;
        Draft = string.Empty;
        Turns.Clear();
        LiveProcesses.Clear();
        PendingPrompts.Clear();
        Attachments.Clear();
        Models.Clear();
        SelectedModel = null;
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
    }

    private void CancelCurrentSession()
    {
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = null;
        _realtimeTask = null;
    }
}
