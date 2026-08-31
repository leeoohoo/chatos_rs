using ChatOS.Core.Domain;
using ChatOS.Presentation.Remote;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Storage.Pickers;

namespace ChatOS.Desktop.Features.Remote;

public sealed partial class RemoteSftpPage : UserControl
{
    public RemoteSftpPage(RemoteSftpViewModel viewModel, LocalizationViewModel localization) { ViewModel = viewModel; Localization = localization; InitializeComponent(); }
    public RemoteSftpViewModel ViewModel { get; }
    public LocalizationViewModel Localization { get; }
    public event EventHandler? CloseRequested;
    public Task OpenAsync(RemoteConnection connection) => ViewModel.OpenAsync(connection);
    private void OnBackClick(object sender, RoutedEventArgs e) => CloseRequested?.Invoke(this, EventArgs.Empty);
    private async void OnEntryClick(object sender, ItemClickEventArgs e) => await ViewModel.OpenEntryCommand.ExecuteAsync(e.ClickedItem as RemoteFileEntry);

    private async void OnCreateDirectoryClick(object sender, RoutedEventArgs e)
    {
        var input = new TextBox { PlaceholderText = Localization.Text("目录名称", "Directory name") };
        var dialog = new ContentDialog { XamlRoot = XamlRoot, Title = Localization.Text("新建远端目录", "New remote directory"), Content = input, PrimaryButtonText = Localization.Create, CloseButtonText = Localization.Cancel };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary) await ViewModel.CreateDirectoryCommand.ExecuteAsync(input.Text);
    }

    private async void OnRenameClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: RemoteFileEntry entry }) return;
        var input = new TextBox { Text = entry.Name };
        var dialog = new ContentDialog { XamlRoot = XamlRoot, Title = Localization.Text($"重命名“{entry.Name}”", $"Rename “{entry.Name}”"), Content = input, PrimaryButtonText = Localization.Rename, CloseButtonText = Localization.Cancel };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary) await ViewModel.RenameCommand.ExecuteAsync(new RemoteSftpRenameRequest(entry, input.Text));
    }

    private async void OnDeleteClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: RemoteFileEntry entry }) return;
        var dialog = new ContentDialog { XamlRoot = XamlRoot, Title = Localization.Text($"删除“{entry.Name}”？", $"Delete “{entry.Name}”?"), Content = entry.IsDirectory ? Localization.Text("目录及其中全部内容会被递归删除。", "The directory and all its contents will be deleted recursively.") : Localization.Text("远端文件会被永久删除。", "The remote file will be permanently deleted."), PrimaryButtonText = Localization.Delete, CloseButtonText = Localization.Cancel, DefaultButton = ContentDialogButton.Close };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary) await ViewModel.DeleteCommand.ExecuteAsync(new RemoteSftpDeleteRequest(entry, entry.IsDirectory));
    }

    private async void OnDownloadClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: RemoteFileEntry entry } || entry.IsDirectory) return;
        var picker = new FileSavePicker { SuggestedFileName = entry.Name };
        picker.FileTypeChoices.Add(Localization.Text("文件", "File"), [Path.GetExtension(entry.Name) is { Length: > 0 } extension ? extension : ".bin"]);
        Initialize(picker);
        var file = await picker.PickSaveFileAsync();
        if (file is null) return;
        await using var stream = await file.OpenStreamForWriteAsync();
        stream.SetLength(0);
        await ViewModel.DownloadAsync(entry, stream);
    }

    private async void OnUploadClick(object sender, RoutedEventArgs e)
    {
        var picker = new FileOpenPicker(); picker.FileTypeFilter.Add("*"); Initialize(picker);
        var file = await picker.PickSingleFileAsync();
        if (file is null) return;
        await using var stream = await file.OpenStreamForReadAsync();
        await ViewModel.UploadAsync(stream, file.Name, false);
        if (ViewModel.ErrorMessage?.Contains("已存在", StringComparison.Ordinal) == true)
        {
            var dialog = new ContentDialog { XamlRoot = XamlRoot, Title = Localization.Text($"覆盖“{file.Name}”？", $"Overwrite “{file.Name}”?"), Content = Localization.Text("远端已存在同名文件。", "A file with the same name already exists on the remote host."), PrimaryButtonText = Localization.Text("覆盖", "Overwrite"), CloseButtonText = Localization.Cancel, DefaultButton = ContentDialogButton.Close };
            if (await dialog.ShowAsync() == ContentDialogResult.Primary) { stream.Position = 0; await ViewModel.UploadAsync(stream, file.Name, true); }
        }
    }

    private void Initialize(object picker)
    {
        var window = (Application.Current as App)?.MainWindow ?? throw new InvalidOperationException(Localization.Text("主窗口尚未创建。", "The main window has not been created."));
        WinRT.Interop.InitializeWithWindow.Initialize(picker, WinRT.Interop.WindowNative.GetWindowHandle(window));
    }
}
