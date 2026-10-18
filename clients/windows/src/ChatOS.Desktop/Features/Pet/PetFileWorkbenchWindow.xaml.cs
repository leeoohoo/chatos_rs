using System.Text;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Desktop.DesignSystem;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Graphics;
using Windows.Storage.Streams;

namespace ChatOS.Desktop.Features.Pet;

public sealed partial class PetFileWorkbenchWindow : Window
{
    private readonly IProjectFilesystemService _filesystem;
    private readonly IUiDispatcher _dispatcher;
    private ProjectFileContent? _file;
    private string _originalContent = string.Empty;
    private bool _isEditing;
    private long _generation;

    public PetFileWorkbenchWindow(
        IProjectFilesystemService filesystem,
        IUiDispatcher dispatcher,
        LocalizationViewModel localization)
    {
        _filesystem = filesystem;
        _dispatcher = dispatcher;
        Localization = localization;
        InitializeComponent();
        AppWindow.Title = Localization.PetFileWorkbench;
        AppWindow.Resize(new SizeInt32(980, 700));
        AppWindow.Closing += OnWindowClosing;
    }

    public LocalizationViewModel Localization { get; }

    public async Task OpenAsync(string path, int? targetLine, CancellationToken cancellationToken = default)
    {
        var generation = Interlocked.Increment(ref _generation);
        await _dispatcher.InvokeAsync(() =>
        {
            Activate();
            ResetForLoading(path);
        }, cancellationToken);
        try
        {
            var file = await _filesystem.ReadFileAsync(path, cancellationToken).ConfigureAwait(false);
            if (generation != _generation) return;
            _file = file;
            _originalContent = file.Content;
            _isEditing = false;
            Task renderTask = Task.CompletedTask;
            await _dispatcher.InvokeAsync(
                () => renderTask = ShowFileAsync(file, targetLine),
                cancellationToken).ConfigureAwait(false);
            await renderTask.ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            if (generation != _generation) return;
            await _dispatcher.InvokeAsync(() => ShowError(exception.Message)).ConfigureAwait(false);
        }
    }

    private void ResetForLoading(string path)
    {
        _file = null;
        _originalContent = string.Empty;
        _isEditing = false;
        FileNameText.Text = Path.GetFileName(path);
        FilePathText.Text = path;
        StatusText.Text = string.Empty;
        HideContent();
        LoadingRing.IsActive = true;
        LoadingRing.Visibility = Visibility.Visible;
    }

    private async Task ShowFileAsync(ProjectFileContent file, int? targetLine)
    {
        LoadingRing.IsActive = false;
        LoadingRing.Visibility = Visibility.Collapsed;
        FileNameText.Text = file.Name;
        FilePathText.Text = file.DisplayPath ?? file.Path;
        StatusText.Text = string.Empty;
        HideContent();

        if (SupportsImagePreview(file))
        {
            try
            {
                var bytes = file.IsBinary ? Convert.FromBase64String(file.Content) : Encoding.UTF8.GetBytes(file.Content);
                using var stream = new InMemoryRandomAccessStream();
                using (var writer = new DataWriter(stream.GetOutputStreamAt(0)))
                {
                    writer.WriteBytes(bytes);
                    await writer.StoreAsync();
                    await writer.FlushAsync();
                }
                stream.Seek(0);
                var image = new BitmapImage();
                await image.SetSourceAsync(stream);
                ImagePreview.Source = image;
                ImagePanel.Visibility = Visibility.Visible;
            }
            catch
            {
                ShowEmpty(
                    Localization.ImagePreviewFailed,
                    Localization.Text("图片数据无效，或系统暂不支持这种格式。", "The image data is invalid or this format is not supported."),
                    "\uEA39");
            }
            return;
        }

        if (file.IsBinary)
        {
            ShowEmpty(
                Localization.BinaryPreviewOnly,
                Localization.BinaryEditorSafetyNotice,
                "\uE7C3");
            return;
        }

        if (IsMarkdown(file.Name) && targetLine is null)
        {
            MarkdownRichTextRenderer.Render(MarkdownPreview, file.Content);
            MarkdownScroll.Visibility = Visibility.Visible;
            return;
        }

        CodePreview.Text = file.Content;
        CodePreview.IsReadOnly = true;
        CodePreview.Visibility = Visibility.Visible;
        UpdateEditControls();
        if (targetLine is > 0)
        {
            SelectLine(CodePreview, targetLine.Value);
        }
    }

    private static void SelectLine(Microsoft.UI.Xaml.Controls.TextBox textBox, int targetLine)
    {
        var text = textBox.Text;
        var start = 0;
        for (var line = 1; line < targetLine && start < text.Length; line++)
        {
            var next = text.IndexOf('\n', start);
            if (next < 0) return;
            start = next + 1;
        }
        var end = text.IndexOf('\n', start);
        if (end < 0) end = text.Length;
        textBox.SelectionStart = start;
        textBox.SelectionLength = Math.Max(0, end - start);
        textBox.Focus(FocusState.Programmatic);
    }

    private void HideContent()
    {
        CodePreview.Visibility = Visibility.Collapsed;
        MarkdownScroll.Visibility = Visibility.Collapsed;
        ImagePanel.Visibility = Visibility.Collapsed;
        ImagePreview.Source = null;
        EmptyState.Visibility = Visibility.Collapsed;
        UpdateEditControls();
    }

    private void ShowEmpty(string title, string description, string glyph)
    {
        EmptyStateTitle.Text = title;
        EmptyStateDescription.Text = description;
        EmptyStateIcon.Glyph = glyph;
        EmptyState.Visibility = Visibility.Visible;
    }

    private void ShowError(string message)
    {
        LoadingRing.IsActive = false;
        LoadingRing.Visibility = Visibility.Collapsed;
        HideContent();
        ShowEmpty(Localization.Text("无法打开文件", "Unable to open file"), message, "\uE783");
    }

    private async void OnRevealClick(object sender, RoutedEventArgs e)
    {
        if (_file is { } file) await _filesystem.OpenExternallyAsync(file.Path, ProjectFileExternalOpenMode.Reveal);
    }

    private async void OnOpenDefaultClick(object sender, RoutedEventArgs e)
    {
        if (_file is { } file) await _filesystem.OpenExternallyAsync(file.Path, ProjectFileExternalOpenMode.Default);
    }

    private async void OnEditClick(object sender, RoutedEventArgs e)
    {
        if (_file is not { IsBinary: false, IsWritable: true }) return;
        _isEditing = true;
        CodePreview.IsReadOnly = false;
        CodePreview.Visibility = Visibility.Visible;
        MarkdownScroll.Visibility = Visibility.Collapsed;
        CodePreview.Focus(FocusState.Programmatic);
        UpdateEditControls();
        await Task.CompletedTask;
    }

    private void OnCancelEditClick(object sender, RoutedEventArgs e)
    {
        if (!_isEditing) return;
        CodePreview.Text = _originalContent;
        _isEditing = false;
        CodePreview.IsReadOnly = true;
        UpdateEditControls();
    }

    private async void OnSaveClick(object sender, RoutedEventArgs e)
    {
        if (!_isEditing || _file is not { IsBinary: false, IsWritable: true }) return;
        SaveButton.IsEnabled = false;
        try
        {
            var disk = await _filesystem.ReadFileAsync(_file.Path);
            if (!string.Equals(disk.Content, _originalContent, StringComparison.Ordinal))
            {
                var conflict = new ContentDialog
                {
                    XamlRoot = Content.XamlRoot,
                    Title = Localization.Text("文件已在其他位置修改", "File changed elsewhere"),
                    Content = Localization.Text("磁盘内容已经变化，仍要覆盖保存吗？", "The file changed on disk. Overwrite it anyway?"),
                    PrimaryButtonText = Localization.Text("覆盖保存", "Overwrite"),
                    CloseButtonText = Localization.Cancel,
                    DefaultButton = ContentDialogButton.Close
                };
                if (await conflict.ShowAsync() != ContentDialogResult.Primary) return;
            }
            await _filesystem.WriteFileAsync(_file.Path, CodePreview.Text);
            _originalContent = CodePreview.Text;
            _file = await _filesystem.ReadFileAsync(_file.Path);
            _isEditing = false;
            CodePreview.IsReadOnly = true;
            StatusText.Text = Localization.Text("已保存。", "Saved.");
            UpdateEditControls();
        }
        catch (Exception exception)
        {
            StatusText.Text = exception.Message;
        }
        finally { SaveButton.IsEnabled = true; }
    }

    private async void OnCloseClick(object sender, RoutedEventArgs e) => await RequestCloseAsync();

    private async Task RequestCloseAsync()
    {
        if (_isEditing && !string.Equals(CodePreview.Text, _originalContent, StringComparison.Ordinal))
        {
            var dialog = new ContentDialog
            {
                XamlRoot = Content.XamlRoot,
                Title = Localization.UnsavedFileChanges,
                Content = Localization.UnsavedFileChangesNotice,
                PrimaryButtonText = Localization.SaveAndContinue,
                SecondaryButtonText = Localization.DiscardChanges,
                CloseButtonText = Localization.Cancel,
                DefaultButton = ContentDialogButton.Primary
            };
            var result = await dialog.ShowAsync();
            if (result == ContentDialogResult.Primary)
            {
                await SaveCurrentAsync();
                if (_isEditing) return;
            }
            else if (result != ContentDialogResult.Secondary) return;
        }
        AppWindow.Hide();
    }

    private async Task SaveCurrentAsync()
    {
        if (!_isEditing || _file is null) return;
        try
        {
            await _filesystem.WriteFileAsync(_file.Path, CodePreview.Text);
            _originalContent = CodePreview.Text;
            _isEditing = false;
            CodePreview.IsReadOnly = true;
            UpdateEditControls();
        }
        catch (Exception exception) { StatusText.Text = exception.Message; }
    }

    private void UpdateEditControls()
    {
        var canEdit = _file is { IsBinary: false, IsWritable: true } && CodePreview.Visibility == Visibility.Visible;
        EditButton.Visibility = canEdit && !_isEditing ? Visibility.Visible : Visibility.Collapsed;
        SaveButton.Visibility = _isEditing ? Visibility.Visible : Visibility.Collapsed;
        CancelEditButton.Visibility = _isEditing ? Visibility.Visible : Visibility.Collapsed;
    }

    private void OnWindowClosing(AppWindow sender, AppWindowClosingEventArgs args)
    {
        args.Cancel = true;
        _ = RequestCloseAsync();
    }

    private static bool IsMarkdown(string name) => Path.GetExtension(name).ToLowerInvariant() is ".md" or ".markdown";

    private static bool SupportsImagePreview(ProjectFileContent file) =>
        file.ContentType?.StartsWith("image/", StringComparison.OrdinalIgnoreCase) == true ||
        Path.GetExtension(file.Name).ToLowerInvariant() is
            ".bmp" or ".gif" or ".ico" or ".jpeg" or ".jpg" or ".png" or
            ".tif" or ".tiff" or ".webp";
}
