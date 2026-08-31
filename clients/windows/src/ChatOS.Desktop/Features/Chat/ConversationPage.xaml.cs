using System.Collections.Specialized;
using System.Text;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Tasks;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage;
using Windows.Storage.Pickers;
using Windows.Storage.Streams;

namespace ChatOS.Desktop.Features.Chat;

public sealed partial class ConversationPage : UserControl
{
    private ScrollViewer? _timelineScrollViewer;
    private bool _isPinnedToBottom = true;

    public ConversationPage(
        ConversationSessionViewModel viewModel,
        MessageTaskGraphViewModel taskGraph,
        LocalizationViewModel localization)
    {
        ViewModel = viewModel;
        TaskGraph = taskGraph;
        Localization = localization;
        InitializeComponent();
        Loaded += OnLoaded;
        ViewModel.Turns.CollectionChanged += OnTurnsChanged;
    }

    public ConversationSessionViewModel ViewModel { get; }

    public MessageTaskGraphViewModel TaskGraph { get; }

    public LocalizationViewModel Localization { get; }

    private async void OnTaskCallbackClicked(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement
            {
                DataContext: ConversationReplyItemViewModel { TaskGraphRequest: { } request },
            })
        {
            await TaskGraph.OpenAsync(request);
        }
    }

    private void OnTaskGraphNodeClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is MessageTaskGraphNodeItemViewModel node)
        {
            TaskGraph.SelectNodeCommand.Execute(node);
        }
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        _timelineScrollViewer ??= FindDescendant<ScrollViewer>(TimelineList);
        if (_timelineScrollViewer is not null)
        {
            _timelineScrollViewer.ViewChanged -= OnTimelineViewChanged;
            _timelineScrollViewer.ViewChanged += OnTimelineViewChanged;
        }

        ScrollToBottom(false);
    }

    private async void OnAddAttachmentClicked(object sender, RoutedEventArgs e)
    {
        try
        {
            var window = (Application.Current as App)?.MainWindow;
            if (window is null)
            {
                ViewModel.AttachmentError = Localization.Text("无法找到当前窗口。", "Unable to find the current window.");
                return;
            }

            var picker = new FileOpenPicker
            {
                ViewMode = PickerViewMode.List,
                SuggestedStartLocation = PickerLocationId.DocumentsLibrary,
            };
            picker.FileTypeFilter.Add("*");
            WinRT.Interop.InitializeWithWindow.Initialize(
                picker,
                WinRT.Interop.WindowNative.GetWindowHandle(window));
            var files = await picker.PickMultipleFilesAsync();
            await AddStorageFilesAsync(files);
        }
        catch (Exception exception)
        {
            ViewModel.AttachmentError = Localization.Text(
                $"无法添加附件：{exception.Message}",
                $"Unable to add attachment: {exception.Message}");
        }
    }

    private void OnComposerDragOver(object sender, DragEventArgs e)
    {
        if (e.DataView.Contains(StandardDataFormats.StorageItems))
        {
            e.AcceptedOperation = DataPackageOperation.Copy;
            e.DragUIOverride.Caption = Localization.Text("添加到当前消息", "Add to current message");
            e.DragUIOverride.IsCaptionVisible = true;
        }
    }

    private async void OnComposerDrop(object sender, DragEventArgs e)
    {
        if (!e.DataView.Contains(StandardDataFormats.StorageItems))
        {
            return;
        }

        try
        {
            var items = await e.DataView.GetStorageItemsAsync();
            await AddStorageFilesAsync(items.OfType<StorageFile>());
        }
        catch (Exception exception)
        {
            ViewModel.AttachmentError = Localization.Text(
                $"无法读取拖入的文件：{exception.Message}",
                $"Unable to read dropped files: {exception.Message}");
        }
    }

    private async void OnComposerPaste(object sender, TextControlPasteEventArgs e)
    {
        DataPackageView content;
        try
        {
            content = Clipboard.GetContent();
        }
        catch (Exception exception)
        {
            ViewModel.AttachmentError = Localization.Text(
                $"无法读取剪贴板：{exception.Message}",
                $"Unable to read the clipboard: {exception.Message}");
            return;
        }

        if (content.Contains(StandardDataFormats.StorageItems))
        {
            e.Handled = true;
            var items = await content.GetStorageItemsAsync();
            await AddStorageFilesAsync(items.OfType<StorageFile>());
            return;
        }

        if (content.Contains(StandardDataFormats.Bitmap))
        {
            e.Handled = true;
            try
            {
                var bitmap = await content.GetBitmapAsync();
                using var stream = await bitmap.OpenReadAsync();
                var data = await ReadAllBytesAsync(stream);
                var mimeType = string.IsNullOrWhiteSpace(stream.ContentType)
                    ? "image/png"
                    : stream.ContentType;
                ViewModel.AddAttachments(new[]
                {
                    ConversationAttachmentDraft.Create(
                        Localization.Text(
                            $"粘贴的图片 {DateTime.Now:yyyy-MM-dd HH.mm.ss}.png",
                            $"Pasted image {DateTime.Now:yyyy-MM-dd HH.mm.ss}.png"),
                        mimeType,
                        ConversationAttachmentKind.Image,
                        ConversationAttachmentOrigin.PastedImage,
                        data),
                });
            }
            catch (Exception exception)
            {
                ViewModel.AttachmentError = Localization.Text(
                    $"无法读取粘贴的图片：{exception.Message}",
                    $"Unable to read pasted image: {exception.Message}");
            }

            return;
        }

        if (!content.Contains(StandardDataFormats.Text))
        {
            return;
        }

        try
        {
            var text = await content.GetTextAsync();
            var dataBytes = Encoding.UTF8.GetBytes(text);
            if (text.Length < 4_000 && dataBytes.Length < 8_000)
            {
                return;
            }

            e.Handled = true;
            ViewModel.AddAttachments(new[]
            {
                ConversationAttachmentDraft.Create(
                    Localization.Text(
                        $"粘贴的长文本 {DateTime.Now:yyyy-MM-dd HH.mm.ss}.txt",
                        $"Pasted long text {DateTime.Now:yyyy-MM-dd HH.mm.ss}.txt"),
                    "text/plain",
                    ConversationAttachmentKind.File,
                    ConversationAttachmentOrigin.PastedText,
                    dataBytes),
            });
        }
        catch (Exception exception)
        {
            ViewModel.AttachmentError = Localization.Text(
                $"无法读取粘贴的文本：{exception.Message}",
                $"Unable to read pasted text: {exception.Message}");
        }
    }

    private void OnRemoveAttachmentClicked(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { DataContext: ConversationAttachmentDraft attachment })
        {
            ViewModel.RemoveAttachmentCommand.Execute(attachment);
        }
    }

    private void OnAskUserSecretChanged(object sender, RoutedEventArgs e)
    {
        if (sender is PasswordBox
            {
                DataContext: AskUserFieldInputViewModel field,
                Password: var password,
            })
        {
            field.Value = password;
        }
    }

    private void OnUnreadNewerClicked(object sender, RoutedEventArgs e)
    {
        ViewModel.MarkNewerContentReadCommand.Execute(null);
        _isPinnedToBottom = true;
        ViewModel.SetViewportPinnedToBottom(true);
        ScrollToBottom(true);
    }

    private async void OnModelSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (sender is ComboBox { SelectedItem: ConversationModelOption model })
        {
            await ViewModel.SelectModelAsync(model);
        }
    }

    private async void OnReasoningToggled(object sender, RoutedEventArgs e)
    {
        if (sender is ToggleSwitch toggle)
        {
            await ViewModel.SetReasoningAsync(toggle.IsOn);
        }
    }

    private async void OnPlanModeToggled(object sender, RoutedEventArgs e)
    {
        if (sender is ToggleSwitch toggle)
        {
            await ViewModel.SetPlanModeAsync(toggle.IsOn);
        }
    }

    private void OnTimelineViewChanged(object? sender, ScrollViewerViewChangedEventArgs e)
    {
        if (_timelineScrollViewer is null)
        {
            return;
        }

        var remaining = _timelineScrollViewer.ScrollableHeight - _timelineScrollViewer.VerticalOffset;
        var pinned = remaining <= 32;
        if (pinned == _isPinnedToBottom)
        {
            return;
        }

        _isPinnedToBottom = pinned;
        ViewModel.SetViewportPinnedToBottom(pinned);
    }

    private void OnTurnsChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        if (_isPinnedToBottom && ViewModel.UnreadNewerCount == 0)
        {
            DispatcherQueue.TryEnqueue(() => ScrollToBottom(false));
        }
    }

    private void ScrollToBottom(bool animated)
    {
        _timelineScrollViewer?.ChangeView(
            null,
            _timelineScrollViewer.ScrollableHeight,
            null,
            !animated);
    }

    private async Task AddStorageFilesAsync(IEnumerable<StorageFile> files)
    {
        var attachments = new List<ConversationAttachmentDraft>();
        var errors = new List<string>();
        foreach (var file in files)
        {
            try
            {
                var properties = await file.GetBasicPropertiesAsync();
                if (properties.Size > ConversationSessionViewModel.MaximumAttachmentBytes)
                {
                    errors.Add($"“{file.Name}”超过 20 MB");
                    continue;
                }

                using var stream = await file.OpenReadAsync();
                var data = await ReadAllBytesAsync(stream);
                var mimeType = string.IsNullOrWhiteSpace(file.ContentType)
                    ? "application/octet-stream"
                    : file.ContentType;
                attachments.Add(ConversationAttachmentDraft.Create(
                    file.Name,
                    mimeType,
                    AttachmentKind(mimeType),
                    ConversationAttachmentOrigin.File,
                    data));
            }
            catch (Exception exception)
            {
                errors.Add($"无法读取“{file.Name}”：{exception.Message}");
            }
        }

        ViewModel.AddAttachments(attachments);
        if (errors.Count > 0)
        {
            if (!string.IsNullOrWhiteSpace(ViewModel.AttachmentError))
            {
                errors.Add(ViewModel.AttachmentError);
            }

            ViewModel.AttachmentError = string.Join("；", errors);
        }
    }

    private static async Task<byte[]> ReadAllBytesAsync(IRandomAccessStream stream)
    {
        if (stream.Size > ConversationSessionViewModel.MaximumAttachmentBytes)
        {
            throw new InvalidOperationException("附件超过 20 MB。 ");
        }

        var bytes = new byte[(int)stream.Size];
        using var reader = new DataReader(stream.GetInputStreamAt(0));
        await reader.LoadAsync((uint)stream.Size);
        reader.ReadBytes(bytes);
        return bytes;
    }

    private static ConversationAttachmentKind AttachmentKind(string mimeType)
    {
        if (mimeType.StartsWith("image/", StringComparison.OrdinalIgnoreCase))
        {
            return ConversationAttachmentKind.Image;
        }

        if (mimeType.StartsWith("audio/", StringComparison.OrdinalIgnoreCase))
        {
            return ConversationAttachmentKind.Audio;
        }

        return ConversationAttachmentKind.File;
    }

    private static T? FindDescendant<T>(DependencyObject root)
        where T : DependencyObject
    {
        var count = VisualTreeHelper.GetChildrenCount(root);
        for (var index = 0; index < count; index++)
        {
            var child = VisualTreeHelper.GetChild(root, index);
            if (child is T result)
            {
                return result;
            }

            var nested = FindDescendant<T>(child);
            if (nested is not null)
            {
                return nested;
            }
        }

        return null;
    }
}
