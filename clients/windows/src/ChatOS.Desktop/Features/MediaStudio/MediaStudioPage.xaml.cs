using ChatOS.Core.Domain;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Storage.Pickers;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class MediaStudioPage : Page
{
    private readonly AppShell.MainWindowViewModel _shell;

    public MediaStudioPage(
        MediaStudioViewModel viewModel,
        AppShell.MainWindowViewModel shell)
    {
        ViewModel = viewModel;
        _shell = shell;
        InitializeComponent();
        ViewModel.PropertyChanged += (_, _) => DispatcherQueue.TryEnqueue(RefreshState);
        ViewModel.LatestImages.CollectionChanged += (_, _) => RefreshState();
        Loaded += OnLoaded;
        RefreshState();
    }

    public MediaStudioViewModel ViewModel { get; }

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        if (_shell.CurrentOwnerUserId is not { Length: > 0 } owner) return;
        await ViewModel.OpenAsync(owner);
        RefreshState();
    }

    private async void OnReloadModelsClick(object sender, RoutedEventArgs e) =>
        await ViewModel.ReloadModelsAsync();

    private async void OnGenerateClick(object sender, RoutedEventArgs e) =>
        await ViewModel.GenerateAsync();

    private async void OnAddReferenceClick(object sender, RoutedEventArgs e)
    {
        var window = (Application.Current as App)?.MainWindow;
        if (window is null) return;
        var picker = new FileOpenPicker();
        picker.FileTypeFilter.Add(".png");
        picker.FileTypeFilter.Add(".jpg");
        picker.FileTypeFilter.Add(".jpeg");
        picker.FileTypeFilter.Add(".webp");
        WinRT.Interop.InitializeWithWindow.Initialize(
            picker,
            WinRT.Interop.WindowNative.GetWindowHandle(window));
        var files = await picker.PickMultipleFilesAsync();
        if (files.Count > 0)
            await ViewModel.AddReferenceImagesAsync(files.Select(file => file.Path).ToArray());
    }

    private void OnRemoveReferenceClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ImageGenerationInput image })
            ViewModel.RemoveReferenceImage(image);
    }

    private void OnHistoryItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is MediaStudioHistoryItem item)
            ViewModel.SelectHistoryItem(item);
    }

    private async void OnSaveImageClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: MediaStudioImageItem image }) return;
        var window = (Application.Current as App)?.MainWindow;
        if (window is null) return;
        var picker = new FileSavePicker
        {
            SuggestedFileName = $"ChatOS-{DateTime.Now:yyyyMMdd-HHmmss}",
        };
        var extension = Path.GetExtension(image.FilePath);
        picker.FileTypeChoices.Add("Image", [extension]);
        WinRT.Interop.InitializeWithWindow.Initialize(
            picker,
            WinRT.Interop.WindowNative.GetWindowHandle(window));
        var target = await picker.PickSaveFileAsync();
        if (target is null) return;
        try
        {
            File.Copy(image.FilePath, target.Path, true);
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            ViewModel.ReportError($"保存图片失败：{exception.Message}");
        }
    }

    private void RefreshState()
    {
        Bindings.Update();
        var hasImages = ViewModel.LatestImages.Count > 0;
        LatestImageGrid.Visibility = hasImages ? Visibility.Visible : Visibility.Collapsed;
        CanvasEmptyState.Visibility = hasImages ? Visibility.Collapsed : Visibility.Visible;
        ErrorInfoBar.IsOpen = !string.IsNullOrWhiteSpace(ViewModel.ErrorMessage);
        ErrorInfoBar.Message = ViewModel.ErrorMessage ?? string.Empty;
    }
}

public sealed class FilePathToImageSourceConverter : IValueConverter
{
    public object? Convert(object value, Type targetType, object parameter, string language)
    {
        if (value is not string { Length: > 0 } path || !File.Exists(path)) return null;
        return new BitmapImage(new Uri(path, UriKind.Absolute));
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        throw new NotSupportedException();
}
