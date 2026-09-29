using System.Text;
using ChatOS.Core.Domain;
using ChatOS.Presentation.AgentTeams;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Storage;
using Windows.Storage.Pickers;
using Windows.Storage.Streams;

namespace ChatOS.Desktop.Features.AgentTeams;

internal static class AgentMessageAttachmentPresenter
{
    private const int TextPreviewLimit = 1_000_000;

    public static async Task PreviewAsync(
        XamlRoot xamlRoot,
        AgentTeamWorkspaceViewModel viewModel,
        AgentMessageAttachment metadata)
    {
        var attachment = await LoadAsync(viewModel, metadata);
        var content = await PreviewContentAsync(attachment);
        var dialog = new ContentDialog
        {
            XamlRoot = xamlRoot,
            Title = attachment.Name,
            Content = content,
            PrimaryButtonText = "另存为",
            CloseButtonText = "关闭",
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
            await SaveLoadedAsync(attachment);
    }

    public static async Task SaveAsync(
        AgentTeamWorkspaceViewModel viewModel,
        AgentMessageAttachment metadata) =>
        await SaveLoadedAsync(await LoadAsync(viewModel, metadata));

    private static async Task<AgentMessageAttachment> LoadAsync(
        AgentTeamWorkspaceViewModel viewModel,
        AgentMessageAttachment metadata) =>
        await viewModel.LoadAttachmentAsync(metadata.Id)
            ?? throw new InvalidOperationException("附件已经不存在。");

    private static async Task<UIElement> PreviewContentAsync(AgentMessageAttachment attachment)
    {
        if (attachment.Kind == AgentMessageAttachmentKind.Image)
        {
            using var stream = new InMemoryRandomAccessStream();
            using (var writer = new DataWriter(stream))
            {
                writer.WriteBytes(attachment.Data);
                await writer.StoreAsync();
                await writer.FlushAsync();
                writer.DetachStream();
            }
            stream.Seek(0);
            var source = new BitmapImage();
            await source.SetSourceAsync(stream);
            return new ScrollViewer
            {
                MaxWidth = 900,
                MaxHeight = 650,
                Content = new Image
                {
                    Source = source,
                    MaxWidth = 860,
                    MaxHeight = 620,
                    Stretch = Stretch.Uniform,
                },
            };
        }

        if (IsText(attachment))
        {
            var byteCount = Math.Min(attachment.Data.Length, TextPreviewLimit);
            var text = Encoding.UTF8.GetString(attachment.Data, 0, byteCount);
            if (attachment.Data.Length > TextPreviewLimit)
                text += "\n\n…预览仅显示前 1 MB，完整内容请另存为。";
            return new TextBox
            {
                Text = text,
                IsReadOnly = true,
                AcceptsReturn = true,
                TextWrapping = TextWrapping.Wrap,
                FontFamily = new FontFamily("Consolas"),
                MinWidth = 620,
                MaxWidth = 860,
                MaxHeight = 620,
            };
        }

        return new StackPanel
        {
            MinWidth = 420,
            Spacing = 8,
            Children =
            {
                new TextBlock { Text = "此附件暂不支持应用内预览。" },
                new TextBlock
                {
                    Text = $"{attachment.MimeType} · {FormatSize(attachment.ByteCount)}",
                    FontSize = 12,
                    Opacity = 0.65,
                },
            },
        };
    }

    private static bool IsText(AgentMessageAttachment attachment) =>
        attachment.MimeType.StartsWith("text/", StringComparison.OrdinalIgnoreCase) ||
        attachment.MimeType.Contains("json", StringComparison.OrdinalIgnoreCase) ||
        attachment.MimeType.Contains("xml", StringComparison.OrdinalIgnoreCase) ||
        Path.GetExtension(attachment.Name).ToLowerInvariant() is
            ".md" or ".txt" or ".csv" or ".log" or ".json" or ".xml" or ".yaml" or ".yml";

    private static async Task SaveLoadedAsync(AgentMessageAttachment attachment)
    {
        var window = (Application.Current as App)?.MainWindow
            ?? throw new InvalidOperationException("无法找到当前窗口。");
        var extension = Path.GetExtension(attachment.Name);
        if (string.IsNullOrWhiteSpace(extension)) extension = ".bin";
        var picker = new FileSavePicker { SuggestedFileName = attachment.Name };
        picker.FileTypeChoices.Add("附件", [extension]);
        WinRT.Interop.InitializeWithWindow.Initialize(
            picker, WinRT.Interop.WindowNative.GetWindowHandle(window));
        var file = await picker.PickSaveFileAsync();
        if (file is not null) await FileIO.WriteBytesAsync(file, attachment.Data);
    }

    private static string FormatSize(long bytes) => bytes switch
    {
        >= 1024 * 1024 => $"{bytes / 1024d / 1024d:0.##} MB",
        >= 1024 => $"{bytes / 1024d:0.##} KB",
        _ => $"{bytes} B",
    };
}
