using ChatOS.Core.Domain;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Media.Core;
using Windows.Media.Playback;
using Windows.Storage.Pickers;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class MediaStudioPage : Page
{
    private readonly AppShell.MainWindowViewModel _shell;
    private string? _activeVideoPath;
    private string? _activeStoryVideoPath;
    private MediaPlaybackList? _storyPlaylist;

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
        ViewModel.History.CollectionChanged += (_, _) => RefreshState();
        ViewModel.VideoHistory.CollectionChanged += (_, _) => RefreshState();
        StoryViewModel.PropertyChanged += (_, _) => DispatcherQueue.TryEnqueue(RefreshState);
        StoryViewModel.Projects.CollectionChanged += (_, _) => RefreshState();
        StoryViewModel.Segments.CollectionChanged += (_, _) => RefreshState();
        Loaded += OnLoaded;
        RefreshState();
    }

    public MediaStudioViewModel ViewModel { get; }

    public StoryStudioViewModel StoryViewModel { get; }

    private void OnStudioSectionChanged(SelectorBar sender, SelectorBarSelectionChangedEventArgs args)
    {
        if (StudioPivot is null || sender.SelectedItem is null) return;
        StudioPivot.SelectedIndex = ReferenceEquals(sender.SelectedItem, ImageSectionItem) ? 0
            : ReferenceEquals(sender.SelectedItem, VideoSectionItem) ? 1
            : ReferenceEquals(sender.SelectedItem, StorySectionItem) ? 2
            : 3;
    }

    private void OnStudioPivotSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (StudioSectionBar is null) return;
        StudioSectionBar.SelectedItem = StudioPivot.SelectedIndex switch
        {
            0 => ImageSectionItem,
            1 => VideoSectionItem,
            2 => StorySectionItem,
            _ => HistorySectionItem,
        };
    }

    private void OnStoryWorkspaceSectionChanged(SelectorBar sender, SelectorBarSelectionChangedEventArgs args)
    {
        if (StoryOverviewScroll is null || StorySegmentsSection is null || sender.SelectedItem is null) return;
        var showSegments = ReferenceEquals(sender.SelectedItem, StorySegmentsSectionItem);
        StoryOverviewScroll.Visibility = showSegments ? Visibility.Collapsed : Visibility.Visible;
        StorySegmentsSection.Visibility = showSegments ? Visibility.Visible : Visibility.Collapsed;
        if (showSegments) return;

        var target = ReferenceEquals(sender.SelectedItem, StoryStyleSectionItem)
            ? StoryStyleSectionAnchor
            : ReferenceEquals(sender.SelectedItem, StoryPortraitsSectionItem)
                ? StoryPortraitsSectionAnchor
                : ReferenceEquals(sender.SelectedItem, StoryGraphSectionItem)
                    ? StoryGraphSectionAnchor
                    : StorySourceSectionAnchor;
        DispatcherQueue.TryEnqueue(() => target.StartBringIntoView());
    }

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        StartupDiagnostics.RecordLifecycle("media studio loaded event entered");
        if (_shell.CurrentOwnerUserId is not { Length: > 0 } owner) return;
        await ViewModel.OpenAsync(owner);
        StartupDiagnostics.RecordLifecycle("media studio image workspace loaded");
        await StoryViewModel.OpenAsync(owner);
        StartupDiagnostics.RecordLifecycle("media studio story workspace loaded");
        RefreshState();
        StartupDiagnostics.RecordLifecycle("media studio loaded event completed");
    }

    private void OnStoryProjectClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is not StoryProjectCard card) return;
        StoryViewModel.OpenProject(card.Project);
        SelectStorySourceSection();
    }

    private void OnStoryLandingProjectClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: StoryProjectCard card }) return;
        StoryViewModel.OpenProject(card.Project);
        SelectStorySourceSection();
    }

    private async void OnLoadMoreStoryProjectsClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.LoadMoreProjectsAsync();

    private async void OnCreateStoryProjectClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.CreateProjectAsync();

    private void OnCloseStoryProjectClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.CloseProject();

    private async void OnSaveStoryProjectClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.SaveCurrentAsync();

    private void OnOpenStoryPromptCenterClick(object sender, RoutedEventArgs e)
    {
        if (StoryWorkspaceSectionBar is not null)
            StoryWorkspaceSectionBar.SelectedItem = StorySourceSectionItem;
        StoryPromptAuditExpander.IsExpanded = true;
        DispatcherQueue.TryEnqueue(() => StoryPromptAuditExpander.StartBringIntoView());
    }

    private void OnQuickSplitStoryClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.QuickSplit();

    private async void OnPlanStoryClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.PlanStoryAsync();

    private async void OnOptimizeStorySourceClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.OptimizeStorySourceAsync();

    private async void OnOptimizeVisualStyleClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.OptimizeVisualStyleAsync();

    private void OnApplyStoryOptimizationClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.ApplyOptimizationSuggestion();

    private void OnDiscardStoryOptimizationClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.ClearOptimizationSuggestion();

    private async void OnResumeStoryPlanningClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.ResumePlanningAsync();

    private void OnPauseStoryPlanningClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.PausePlanning();

    private async void OnAbandonStoryPlanningClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.AbandonPlanningAsync();

    private async void OnLoadMorePlanningRunsClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.LoadMorePlanningRunsAsync();

    private void OnAddStorySegmentClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.AddSegment();

    private void OnMoveStorySegmentUpClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.MoveSelectedSegmentUp();

    private void OnMoveStorySegmentDownClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.MoveSelectedSegmentDown();

    private void OnRemoveStorySegmentClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.RemoveSelectedSegment();

    private async void OnRefineStorySegmentClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.RefineSelectedSegmentAsync();

    private async void OnApplyStorySegmentRefinementClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.ApplySegmentRefinementAsync();

    private void OnDiscardStorySegmentRefinementClick(object sender, RoutedEventArgs e) =>
        StoryViewModel.ClearSegmentRefinement();

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

    private async void OnAutoFillStoryContinuityClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.AutoFillContinuityAsync();

    private void OnStoryContinuityIssueClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: StoryContinuityIssue issue })
            StoryViewModel.SelectContinuityIssue(issue);
    }

    private void OnPlayStoryPlaylistClick(object sender, RoutedEventArgs e)
    {
        var videos = StoryViewModel.StartStoryPlaylist();
        if (videos.Count == 0) return;
        ReleaseStoryPlaylist();
        var playlist = new MediaPlaybackList();
        foreach (var video in videos)
        {
            playlist.Items.Add(new MediaPlaybackItem(
                MediaSource.CreateFromUri(new Uri(video.FilePath, UriKind.Absolute))));
        }
        playlist.CurrentItemChanged += OnStoryPlaylistItemChanged;
        _storyPlaylist = playlist;
        _activeStoryVideoPath = "playlist";
        StoryVideoPlayer.Source = playlist;
        StoryVideoPlayer.Visibility = Visibility.Visible;
        StoryVideoPlayer.MediaPlayer.Play();
    }

    private void OnStopStoryPlaylistClick(object sender, RoutedEventArgs e)
    {
        StoryViewModel.StopStoryPlaylist();
        ReleaseStoryPlaylist();
        _activeStoryVideoPath = null;
        StoryVideoPlayer.Source = null;
        RefreshStoryState();
    }

    private void OnStoryProjectVideoClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is not StoryProjectVideoItem item) return;
        StoryViewModel.SelectProjectVideo(item);
        ReleaseStoryPlaylist();
        _activeStoryVideoPath = null;
        RefreshStoryState();
        StoryVideoPlayer.MediaPlayer.Play();
    }

    private void OnStoryProjectImageClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is StoryProjectImageItem item)
            StoryViewModel.SelectProjectImage(item);
    }

    private void OnStoryPlaylistItemChanged(
        MediaPlaybackList sender,
        CurrentMediaPlaybackItemChangedEventArgs args)
    {
        if (args.NewItem is null)
        {
            DispatcherQueue.TryEnqueue(() => StoryViewModel.StopStoryPlaylist());
            return;
        }
        var index = sender.Items.IndexOf(args.NewItem);
        if (index >= 0)
            DispatcherQueue.TryEnqueue(() => StoryViewModel.SelectPlaylistIndex(index));
    }

    private void ReleaseStoryPlaylist()
    {
        if (_storyPlaylist is not null)
            _storyPlaylist.CurrentItemChanged -= OnStoryPlaylistItemChanged;
        _storyPlaylist = null;
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

    private async void OnExtractStoryVideoLastFrameClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.ExtractSelectedVideoLastFrameAsync();

    private async void OnGenerateStoryVideoClick(object sender, RoutedEventArgs e)
    {
        var segment = StoryViewModel.SelectedSegment;
        if (segment is null)
            return;

        if (segment.VideoPath is not { Length: > 0 })
        {
            if (!StoryViewModel.CanGenerateFromPreviousVideo)
            {
                await StoryViewModel.GenerateVideoAsync();
                return;
            }
            var guidanceDialog = new ContentDialog
            {
                XamlRoot = XamlRoot,
                Title = "选择视频连续性输入",
                Content = StoryViewModel.PreviousVideoGenerationSummary,
                PrimaryButtonText = "延续上一段成片",
                SecondaryButtonText = "仅按本段首尾帧",
                CloseButtonText = "取消",
                DefaultButton = ContentDialogButton.Primary,
            };
            var guidanceResult = await guidanceDialog.ShowAsync();
            if (guidanceResult == ContentDialogResult.Primary)
                await StoryViewModel.GenerateVideoFromPreviousAsync(segment.Id);
            else if (guidanceResult == ContentDialogResult.Secondary)
                await StoryViewModel.GenerateVideoAsync();
            return;
        }

        var canUseReference = StoryViewModel.CanRegenerateFromCurrentVideo;
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = "确认重新生成已完成视频？",
            Content = StoryViewModel.VideoRegenerationSummary,
            PrimaryButtonText = canUseReference
                ? "参考当前视频重做"
                : "仅按首尾帧重做",
            SecondaryButtonText = canUseReference
                ? "仅按首尾帧重做"
                : string.Empty,
            CloseButtonText = "保留当前视频",
            DefaultButton = ContentDialogButton.Close,
        };
        var result = await dialog.ShowAsync();
        if (result == ContentDialogResult.Primary)
            await StoryViewModel.RegenerateSelectedVideoAsync(segment.Id, canUseReference);
        else if (result == ContentDialogResult.Secondary)
            await StoryViewModel.RegenerateSelectedVideoAsync(segment.Id, false);
    }

    private async void OnResumeStoryVideoClick(object sender, RoutedEventArgs e) =>
        await StoryViewModel.ResumeSelectedVideoAsync();

    private async void OnAbandonStoryVideoJobClick(object sender, RoutedEventArgs e)
    {
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = "确认清除视频任务记录？",
            Content = "ChatOS 将不再查询这个服务端任务。只有确认不需要旧任务时才清除；之后重新提交可能再次计费。",
            PrimaryButtonText = "清除本机记录",
            CloseButtonText = "保留并返回",
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
            await StoryViewModel.AbandonSelectedVideoJobAsync();
    }

    private async void OnRestoreStoryVideoVersionClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: StoryArchivedVideoEditor archived })
            await StoryViewModel.RestoreArchivedVideoAsync(archived);
    }

    private async void OnRestoreStoryFrameVersionClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: StoryArchivedFrameEditor archived })
            await StoryViewModel.RestoreArchivedFrameAsync(archived);
    }

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

    private void OnLatestImageGridSizeChanged(object sender, SizeChangedEventArgs e)
    {
        if (sender is not GridView { ItemsPanelRoot: ItemsWrapGrid panel } grid) return;
        var available = Math.Max(280, grid.ActualWidth - 32);
        var columns = available >= 620 ? 2 : 1;
        panel.ItemWidth = Math.Max(280, Math.Floor((available - (columns - 1) * 12) / columns));
    }

    private void OnPromptSuggestionClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string prompt }) ViewModel.Prompt = prompt;
    }

    private async void OnPreviewImageClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: MediaStudioImageItem image }) return;
        var preview = new Image
        {
            Source = new FilePathToImageSourceConverter().Convert(
                image.FilePath,
                typeof(BitmapImage),
                null!,
                string.Empty) as BitmapImage,
            Stretch = Microsoft.UI.Xaml.Media.Stretch.Uniform,
            MaxWidth = 980,
            MaxHeight = 700,
        };
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = image.Name,
            Content = new ScrollViewer
            {
                Content = preview,
                MinWidth = 680,
                MinHeight = 420,
                HorizontalScrollBarVisibility = ScrollBarVisibility.Auto,
                VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            },
            CloseButtonText = "关闭",
        };
        await dialog.ShowAsync();
    }

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

    private async void OnAddGeneratedReferenceClick(object sender, RoutedEventArgs e)
    {
        var remaining = Math.Max(0, 8 - ViewModel.ReferenceImages.Count);
        var images = ViewModel.History
            .SelectMany(item => item.Images)
            .Where(item => File.Exists(item.FilePath))
            .ToArray();
        if (remaining == 0 || images.Length == 0)
        {
            var emptyDialog = new ContentDialog
            {
                XamlRoot = XamlRoot,
                Title = remaining == 0 ? "参考图已达上限" : "没有可用的图片记录",
                Content = remaining == 0
                    ? "最多可以添加 8 张参考图。"
                    : "请先生成图片，或从本机添加参考图。",
                CloseButtonText = "知道了",
            };
            await emptyDialog.ShowAsync();
            return;
        }

        var list = CreateGeneratedImagePicker(images, ListViewSelectionMode.Multiple);
        var content = new StackPanel { Spacing = 10 };
        content.Children.Add(new TextBlock
        {
            Text = "按生成顺序选择，可一次添加多张图片。",
            TextWrapping = TextWrapping.Wrap,
        });
        content.Children.Add(list);
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = $"从创作记录选择参考图（最多 {remaining} 张）",
            Content = content,
            PrimaryButtonText = "添加",
            CloseButtonText = "取消",
            DefaultButton = ContentDialogButton.Primary,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;
        var paths = list.SelectedItems
            .OfType<MediaStudioImageItem>()
            .Take(remaining)
            .Select(item => item.FilePath)
            .ToArray();
        if (paths.Length > 0) await ViewModel.AddReferenceImagesAsync(paths);
    }

    private void OnRemoveReferenceClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: MediaStudioReferenceImage image })
            ViewModel.RemoveReferenceImage(image);
    }

    private void OnClearReferencesClick(object sender, RoutedEventArgs e) =>
        ViewModel.ClearReferenceImages();

    private void OnImageHistoryCardClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: MediaStudioHistoryItem item })
        {
            ViewModel.SelectHistoryItem(item);
            StudioPivot.SelectedIndex = 0;
        }
    }

    private void OnStoryHistoryProjectClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: StoryProjectCard card }) return;
        StoryViewModel.OpenProject(card.Project);
        SelectStorySourceSection();
        StudioPivot.SelectedIndex = 2;
    }

    private void OnStoryHistoryPlayClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: StoryProjectCard card } || !card.CanPlay) return;
        StoryViewModel.OpenProject(card.Project);
        StudioPivot.SelectedIndex = 2;
        if (StoryWorkspaceSectionBar is not null)
            StoryWorkspaceSectionBar.SelectedItem = StoryGraphSectionItem;
        DispatcherQueue.TryEnqueue(() =>
        {
            RefreshStoryState();
            OnPlayStoryPlaylistClick(sender, e);
        });
    }

    private void SelectStorySourceSection()
    {
        if (StoryWorkspaceSectionBar is not null)
            StoryWorkspaceSectionBar.SelectedItem = StorySourceSectionItem;
    }

    private async void OnLoadMoreHistoryClick(object sender, RoutedEventArgs e) =>
        await ViewModel.LoadMoreHistoryAsync();

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

    private async void OnPickGeneratedVideoFrameClick(object sender, RoutedEventArgs e)
    {
        var images = ViewModel.History
            .SelectMany(item => item.Images)
            .Where(item => File.Exists(item.FilePath))
            .ToArray();
        if (images.Length == 0)
        {
            var emptyDialog = new ContentDialog
            {
                XamlRoot = XamlRoot,
                Title = "没有可用的图片记录",
                Content = "请先生成图片，或从本机选择首帧参考图。",
                CloseButtonText = "知道了",
            };
            await emptyDialog.ShowAsync();
            return;
        }

        var list = CreateGeneratedImagePicker(images, ListViewSelectionMode.Single);
        var content = new StackPanel { Spacing = 10 };
        content.Children.Add(new TextBlock
        {
            Text = "选择一张已生成图片作为视频首帧；选择后不会自动开始生成。",
            TextWrapping = TextWrapping.Wrap,
        });
        content.Children.Add(list);
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = "从创作记录选择视频首帧",
            Content = content,
            PrimaryButtonText = "使用",
            CloseButtonText = "取消",
            DefaultButton = ContentDialogButton.Primary,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary &&
            list.SelectedItem is MediaStudioImageItem selected)
        {
            await ViewModel.SetVideoFirstFrameAsync(selected.FilePath);
        }
    }

    private ListView CreateGeneratedImagePicker(
        IReadOnlyList<MediaStudioImageItem> images,
        ListViewSelectionMode selectionMode) =>
        new()
        {
            ItemsSource = images,
            ItemTemplate = (DataTemplate)Resources["GeneratedImagePickerItemTemplate"],
            SelectionMode = selectionMode,
            MaxHeight = 460,
            MinWidth = 560,
        };

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

    private void OnVideoHistoryCardClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: MediaStudioVideoHistoryItem item })
        {
            ViewModel.SelectVideoHistoryItem(item);
            StudioPivot.SelectedIndex = 1;
        }
    }

    private async void OnLoadMoreVideoHistoryClick(object sender, RoutedEventArgs e) =>
        await ViewModel.LoadMoreVideoHistoryAsync();

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
        var hasImages = ViewModel.LatestImages.Count > 0;
        LatestImageGrid.Visibility = hasImages ? Visibility.Visible : Visibility.Collapsed;
        LatestImagePromptPanel.Visibility = hasImages ? Visibility.Visible : Visibility.Collapsed;
        CanvasEmptyState.Visibility = hasImages ? Visibility.Collapsed : Visibility.Visible;
        var error = StoryViewModel.ErrorMessage ?? ViewModel.ErrorMessage;
        ErrorInfoBar.IsOpen = !string.IsNullOrWhiteSpace(error);
        ErrorInfoBar.Message = error ?? string.Empty;
        RefreshHistoryState();
        RefreshVideoState();
        RefreshStoryState();
    }

    private void RefreshHistoryState()
    {
        if (HistoryEmptyState is null) return;
        var isLoading = ViewModel.IsInitialLoading || StoryViewModel.IsInitialLoading;
        var hasStories = StoryViewModel.Projects.Count > 0 || StoryViewModel.HasMoreProjects;
        var hasVideos = ViewModel.VideoHistory.Count > 0 || ViewModel.HasMoreVideoHistory;
        var hasImages = ViewModel.History.Count > 0 || ViewModel.HasMoreHistory;
        HistoryLoadingState.Visibility = isLoading ? Visibility.Visible : Visibility.Collapsed;
        HistoryEmptyState.Visibility = isLoading || hasStories || hasVideos || hasImages
            ? Visibility.Collapsed
            : Visibility.Visible;
        StoryHistorySection.Visibility = !isLoading && hasStories ? Visibility.Visible : Visibility.Collapsed;
        VideoHistorySection.Visibility = !isLoading && hasVideos ? Visibility.Visible : Visibility.Collapsed;
        ImageHistorySection.Visibility = !isLoading && hasImages ? Visibility.Visible : Visibility.Collapsed;
    }

    private void RefreshVideoState()
    {
        var videoPath = ViewModel.LatestVideo?.FilePath;
        var hasVideo = videoPath is { Length: > 0 } && File.Exists(videoPath);
        var hasFailed = string.Equals(ViewModel.VideoProgress?.Status, "failed", StringComparison.OrdinalIgnoreCase);
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
        VideoResultPanel.Visibility = hasVideo && !ViewModel.IsGeneratingVideo
            ? Visibility.Visible
            : Visibility.Collapsed;
        VideoEmptyState.Visibility = !hasVideo && !ViewModel.IsGeneratingVideo && !hasFailed
            ? Visibility.Visible
            : Visibility.Collapsed;
        VideoFailedState.Visibility = hasFailed ? Visibility.Visible : Visibility.Collapsed;
        VideoProgressPanel.Visibility = ViewModel.IsGeneratingVideo
            ? Visibility.Visible
            : Visibility.Collapsed;
        VideoCancelButton.Visibility = ViewModel.IsGeneratingVideo
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
        var workspaceOpen = StoryViewModel.IsWorkspaceOpen;
        StoryCreatePanel.Visibility = workspaceOpen
            ? Visibility.Collapsed
            : Visibility.Visible;
        StoryWorkspacePanel.Visibility = workspaceOpen
            ? Visibility.Visible
            : Visibility.Collapsed;
        StoryProjectSidebar.Visibility = workspaceOpen ? Visibility.Visible : Visibility.Collapsed;
        Grid.SetColumn(StoryMainPanel, workspaceOpen ? 1 : 0);
        Grid.SetColumnSpan(StoryMainPanel, workspaceOpen ? 1 : 2);
        var hasProjects = StoryViewModel.Projects.Count > 0;
        var isStoryLoading = ViewModel.IsInitialLoading || StoryViewModel.IsInitialLoading;
        StoryLandingLoadingState.Visibility = isStoryLoading ? Visibility.Visible : Visibility.Collapsed;
        StoryLandingEmptyState.Visibility = !isStoryLoading && !hasProjects ? Visibility.Visible : Visibility.Collapsed;
        StoryLandingProjects.Visibility = !isStoryLoading && hasProjects ? Visibility.Visible : Visibility.Collapsed;
        StoryCreateForm.Visibility = isStoryLoading ? Visibility.Collapsed : Visibility.Visible;
        var hasSegment = StoryViewModel.SelectedSegment is not null;
        StorySegmentEmptyState.Visibility = hasSegment ? Visibility.Collapsed : Visibility.Visible;
        StorySegmentEditorPanel.Visibility = hasSegment ? Visibility.Visible : Visibility.Collapsed;
        var hasResource = StoryViewModel.SelectedResource is not null;
        StoryResourceEmptyState.Visibility = hasResource ? Visibility.Collapsed : Visibility.Visible;
        StoryResourceEditorPanel.Visibility = hasResource ? Visibility.Visible : Visibility.Collapsed;

        if (StoryViewModel.IsPlaylistActive)
        {
            StoryVideoPlayer.Visibility = Visibility.Visible;
            return;
        }
        ReleaseStoryPlaylist();

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
