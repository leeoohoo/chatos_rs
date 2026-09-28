using ChatOS.Core.Domain;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Media.Core;
using Windows.Storage.Pickers;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class MediaStudioPage : Page
{
    private readonly AppShell.MainWindowViewModel _shell;
    private string? _activeVideoPath;
    private string? _activeStoryVideoPath;

    public MediaStudioPage(
        MediaStudioViewModel viewModel,
        StoryStudioViewModel storyViewModel,
        AppShell.MainWindowViewModel shell)
    {
        ViewModel = viewModel;
        StoryViewModel = storyViewModel;
        _shell = shell;
        InitializeComponent();
        ViewModel.PropertyChanged += (_, _) => DispatcherQueue.TryEnqueue(RefreshState);
        ViewModel.LatestImages.CollectionChanged += (_, _) => RefreshState();
        StoryViewModel.PropertyChanged += (_, _) => DispatcherQueue.TryEnqueue(RefreshState);
        StoryViewModel.Projects.CollectionChanged += (_, _) => RefreshState();
        StoryViewModel.Segments.CollectionChanged += (_, _) => RefreshState();
        Loaded += OnLoaded;
        RefreshState();
    }

    public MediaStudioViewModel ViewModel { get; }

    public StoryStudioViewModel StoryViewModel { get; }

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        if (_shell.CurrentOwnerUserId is not { Length: > 0 } owner) return;
        await Task.WhenAll(ViewModel.OpenAsync(owner), StoryViewModel.OpenAsync(owner));
        RefreshState();
    }

    private void OnStoryProjectClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is StoryProjectCard card) StoryViewModel.OpenProject(card.Project);
    }

    private async void OnCreateStoryProjectClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.CreateProjectAsync();

    private void OnCloseStoryProjectClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.CloseProject();

    private async void OnSaveStoryProjectClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.SaveCurrentAsync();

    private void OnQuickSplitStoryClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.QuickSplit();

    private async void OnPlanStoryClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.PlanStoryAsync();

    private void OnAddStorySegmentClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.AddSegment();

    private void OnRemoveStorySegmentClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.RemoveSelectedSegment();

    private void OnAddStoryResourceClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.AddResource();

    private void OnRemoveStoryResourceClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.RemoveSelectedResource();

    private void OnStoryRelationResourceClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: StoryResourceRelationGroup group })
            StoryViewModel.SelectRelationResource(group);
    }

    private void OnStoryRelationSegmentClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: StorySegmentRelationLink link })
            StoryViewModel.SelectRelationSegment(link);
    }

    private async void OnGenerateStoryResourceImageClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.GenerateResourceImageAsync();

    private async void OnImportStoryResourceImageClick(object sender, RoutedEventArgs e)
    {
        if (await PickStoryAssetAsync(false) is { } path)
            await StoryViewModel.ImportResourceImageAsync(path);
    }

    private async void OnGenerateStoryFirstFrameClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.GenerateFirstFrameAsync();

    private async void OnImportStoryFirstFrameClick(object sender, RoutedEventArgs e)
    {
        if (await PickStoryAssetAsync(false) is { } path)
            await StoryViewModel.ImportFirstFrameAsync(path);
    }

    private async void OnGenerateStoryLastFrameClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.GenerateLastFrameAsync();

    private async void OnImportStoryLastFrameClick(object sender, RoutedEventArgs e)
    {
        if (await PickStoryAssetAsync(false) is { } path)
            await StoryViewModel.ImportLastFrameAsync(path);
    }

    private async void OnGenerateStoryVideoClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.GenerateVideoAsync();

    private async void OnImportStoryVideoClick(object sender, RoutedEventArgs e)
    {
        if (await PickStoryAssetAsync(true) is { } path)
            await StoryViewModel.ImportVideoAsync(path);
    }

    private async void OnRunStoryBatchClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.RunBatchAsync();

    private void OnCancelStoryGenerationClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.CancelGeneration();

    private static async Task<string?> PickStoryAssetAsync(bool video)
    {
        var window = (Application.Current as App)?.MainWindow;
        if (window is null) return null;
        var picker = new FileOpenPicker();
        if (video)
        {
            picker.FileTypeFilter.Add(".mp4");
            picker.FileTypeFilter.Add(".mov");
        }
        else
        {
            picker.FileTypeFilter.Add(".png");
            picker.FileTypeFilter.Add(".jpg");
            picker.FileTypeFilter.Add(".jpeg");
            picker.FileTypeFilter.Add(".webp");
        }
        WinRT.Interop.InitializeWithWindow.Initialize(
            picker,
            WinRT.Interop.WindowNative.GetWindowHandle(window));
        return (await picker.PickSingleFileAsync())?.Path;
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

    private async void OnUseImageForVideoClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: MediaStudioImageItem image })
        {
            await ViewModel.SetVideoFirstFrameAsync(image.FilePath);
            StudioPivot.SelectedIndex = 1;
        }
    }

    private async void OnPickVideoFrameClick(object sender, RoutedEventArgs e)
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
        var file = await picker.PickSingleFileAsync();
        if (file is not null) await ViewModel.SetVideoFirstFrameAsync(file.Path);
    }

    private void OnRemoveVideoFrameClick(object sender, RoutedEventArgs e) =>
        ViewModel.RemoveVideoFirstFrame();

    private async void OnPickVideoAudioClick(object sender, RoutedEventArgs e)
    {
        var window = (Application.Current as App)?.MainWindow;
        if (window is null) return;
        var picker = new FileOpenPicker();
        picker.FileTypeFilter.Add(".mp3");
        picker.FileTypeFilter.Add(".wav");
        picker.FileTypeFilter.Add(".m4a");
        picker.FileTypeFilter.Add(".aac");
        WinRT.Interop.InitializeWithWindow.Initialize(
            picker,
            WinRT.Interop.WindowNative.GetWindowHandle(window));
        var file = await picker.PickSingleFileAsync();
        if (file is not null) await ViewModel.SetVideoReferenceAudioAsync(file.Path);
    }

    private void OnRemoveVideoAudioClick(object sender, RoutedEventArgs e) =>
        ViewModel.RemoveVideoReferenceAudio();

    private async void OnGenerateVideoClick(object sender, RoutedEventArgs e) =>
        await ViewModel.GenerateVideoAsync();

    private void OnCancelVideoClick(object sender, RoutedEventArgs e) =>
        ViewModel.CancelVideoGeneration();

    private void OnVideoHistoryItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is MediaStudioVideoHistoryItem item)
            ViewModel.SelectVideoHistoryItem(item);
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

    private async void OnSaveVideoClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: MediaStudioVideoHistoryItem video }) return;
        var window = (Application.Current as App)?.MainWindow;
        if (window is null) return;
        var picker = new FileSavePicker
        {
            SuggestedFileName = $"ChatOS-video-{DateTime.Now:yyyyMMdd-HHmmss}",
        };
        var extension = Path.GetExtension(video.FilePath);
        picker.FileTypeChoices.Add("Video", [extension]);
        WinRT.Interop.InitializeWithWindow.Initialize(
            picker,
            WinRT.Interop.WindowNative.GetWindowHandle(window));
        var target = await picker.PickSaveFileAsync();
        if (target is null) return;
        try
        {
            File.Copy(video.FilePath, target.Path, true);
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            ViewModel.ReportError($"保存视频失败：{exception.Message}");
        }
    }

    private void RefreshState()
    {
        Bindings.Update();
        var hasImages = ViewModel.LatestImages.Count > 0;
        LatestImageGrid.Visibility = hasImages ? Visibility.Visible : Visibility.Collapsed;
        CanvasEmptyState.Visibility = hasImages ? Visibility.Collapsed : Visibility.Visible;
        var error = StoryViewModel.ErrorMessage ?? ViewModel.ErrorMessage;
        ErrorInfoBar.IsOpen = !string.IsNullOrWhiteSpace(error);
        ErrorInfoBar.Message = error ?? string.Empty;
        RefreshVideoState();
        RefreshStoryState();
    }

    private void RefreshVideoState()
    {
        var videoPath = ViewModel.LatestVideo?.FilePath;
        var hasVideo = videoPath is { Length: > 0 } && File.Exists(videoPath);
        if (hasVideo && !string.Equals(_activeVideoPath, videoPath, StringComparison.OrdinalIgnoreCase))
        {
            _activeVideoPath = videoPath;
            VideoPlayer.Source = MediaSource.CreateFromUri(new Uri(videoPath!, UriKind.Absolute));
        }
        else if (!hasVideo && _activeVideoPath is not null)
        {
            _activeVideoPath = null;
            VideoPlayer.Source = null;
        }
        VideoPlayer.Visibility = hasVideo && !ViewModel.IsGeneratingVideo
            ? Visibility.Visible
            : Visibility.Collapsed;
        VideoEmptyState.Visibility = !hasVideo && !ViewModel.IsGeneratingVideo
            ? Visibility.Visible
            : Visibility.Collapsed;
        VideoProgressPanel.Visibility = ViewModel.IsGeneratingVideo
            ? Visibility.Visible
            : Visibility.Collapsed;
        if (ViewModel.VideoProgress?.Percent is { } percent)
        {
            VideoProgressBar.IsIndeterminate = false;
            VideoProgressBar.Value = Math.Clamp(percent, 0, 100);
        }
        else
        {
            VideoProgressBar.IsIndeterminate = true;
        }
    }

    private void RefreshStoryState()
    {
        StoryCreatePanel.Visibility = StoryViewModel.IsWorkspaceOpen
            ? Visibility.Collapsed
            : Visibility.Visible;
        StoryWorkspacePanel.Visibility = StoryViewModel.IsWorkspaceOpen
            ? Visibility.Visible
            : Visibility.Collapsed;
        var hasSegment = StoryViewModel.SelectedSegment is not null;
        StorySegmentEmptyState.Visibility = hasSegment ? Visibility.Collapsed : Visibility.Visible;
        StorySegmentEditorPanel.Visibility = hasSegment ? Visibility.Visible : Visibility.Collapsed;
        var hasResource = StoryViewModel.SelectedResource is not null;
        StoryResourceEmptyState.Visibility = hasResource ? Visibility.Collapsed : Visibility.Visible;
        StoryResourceEditorPanel.Visibility = hasResource ? Visibility.Visible : Visibility.Collapsed;

        var path = StoryViewModel.SelectedSegment?.VideoPath;
        var hasVideo = path is { Length: > 0 } && File.Exists(path);
        if (hasVideo && !string.Equals(_activeStoryVideoPath, path, StringComparison.OrdinalIgnoreCase))
        {
            _activeStoryVideoPath = path;
            StoryVideoPlayer.Source = MediaSource.CreateFromUri(new Uri(path!, UriKind.Absolute));
        }
        else if (!hasVideo && _activeStoryVideoPath is not null)
        {
            _activeStoryVideoPath = null;
            StoryVideoPlayer.Source = null;
        }
        StoryVideoPlayer.Visibility = hasVideo ? Visibility.Visible : Visibility.Collapsed;
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
