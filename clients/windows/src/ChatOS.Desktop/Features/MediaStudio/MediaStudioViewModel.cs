using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class MediaStudioViewModel : ObservableObject
{
    public static readonly IReadOnlyList<string> AvailableSizes =
        ["auto", "1024x1024", "1536x1024", "1024x1536"];
    public static readonly IReadOnlyList<int> AvailableCounts = [1, 2, 3, 4];
    private const int MaximumImageBytes = 20 * 1024 * 1024;
    private readonly IMediaGenerationService _service;
    private readonly MediaStudioHistoryStore _historyStore;
    private string? _ownerUserId;
    private bool _initialized;
    private CancellationTokenSource? _videoGenerationCancellation;
    private MediaStudioHistoryCursor? _historyCursor;
    private MediaStudioHistoryCursor? _videoHistoryCursor;
    private Guid _session = Guid.NewGuid();

    public MediaStudioViewModel(
        IMediaGenerationService service,
        MediaStudioHistoryStore historyStore)
    {
        _service = service;
        _historyStore = historyStore;
        ReferenceImages.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(HasReferenceImages));
            OnPropertyChanged(nameof(CanAddReferenceImages));
            OnPropertyChanged(nameof(CanClearReferenceImages));
            OnPropertyChanged(nameof(ReferenceImageCountLabel));
            OnPropertyChanged(nameof(GenerateImageActionLabel));
            OnPropertyChanged(nameof(ImageEmptyDescription));
        };
        Models.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(HasImageModels));
            OnPropertyChanged(nameof(ShowImageModelLoading));
            OnPropertyChanged(nameof(ShowImageModelEmpty));
        };
        VideoModels.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(HasVideoModels));
            OnPropertyChanged(nameof(ShowVideoModelLoading));
            OnPropertyChanged(nameof(ShowVideoModelEmpty));
        };
    }

    public ObservableCollection<MediaGenerationModel> Models { get; } = [];

    public ObservableCollection<MediaGenerationModel> VideoModels { get; } = [];

    public ObservableCollection<MediaStudioHistoryItem> History { get; } = [];

    public ObservableCollection<MediaStudioVideoHistoryItem> VideoHistory { get; } = [];

    public ObservableCollection<MediaStudioImageItem> LatestImages { get; } = [];

    public ObservableCollection<MediaStudioReferenceImage> ReferenceImages { get; } = [];

    public IReadOnlyList<string> Sizes => AvailableSizes;

    public IReadOnlyList<int> Counts => AvailableCounts;

    public IReadOnlyList<string> VideoSizes => VideoProfile.Sizes;

    public IReadOnlyList<int> VideoDurations => VideoProfile.Durations;

    public IReadOnlyList<string> VideoRatios => VideoGenerationProfile.Ratios;

    public bool CanUseVideoReferenceAudio => VideoProfile.SupportsReferenceVideo;

    public bool CanGenerate => !IsBusy && SelectedModel is not null &&
        !string.IsNullOrWhiteSpace(Prompt) && _ownerUserId is not null;

    public bool CanGenerateVideo => !IsGeneratingVideo && SelectedVideoModel is not null &&
        !string.IsNullOrWhiteSpace(VideoPrompt) && _ownerUserId is not null;

    public bool HasImageModels => Models.Count > 0;

    public bool HasVideoModels => VideoModels.Count > 0;

    public bool ShowImageModelLoading => IsLoadingModels && !HasImageModels;

    public bool ShowVideoModelLoading => IsLoadingModels && !HasVideoModels;

    public bool ShowImageModelEmpty => !IsLoadingModels && !HasImageModels;

    public bool ShowVideoModelEmpty => !IsLoadingModels && !HasVideoModels;

    public bool HasReferenceImages => ReferenceImages.Count > 0;

    public bool CanAddReferenceImages => !IsBusy && ReferenceImages.Count < 8;

    public bool CanClearReferenceImages => !IsBusy && HasReferenceImages;

    public string ReferenceImageCountLabel => $"{ReferenceImages.Count} / 8";

    public string GenerateImageActionLabel => IsGeneratingImage
        ? "正在生成…"
        : HasReferenceImages
            ? "基于参考图生成"
            : "生成图片";

    public string ImageEmptyDescription => HasReferenceImages
        ? "参考图已经添加。请在右侧描述希望保留、修改或重新设计的内容。"
        : "选择图片模型、填写提示词并生成；也可以添加参考图进行图生图。";

    public string GenerateVideoActionLabel => VideoFirstFrame is null
        ? "生成视频"
        : "基于首帧生成视频";

    public string ImageCanvasSubtitle => IsGeneratingImage
        ? "正在生成，完成后会自动保存到本机"
        : LatestImageBatch is { } latest
            ? $"{latest.ModelName} · {latest.Images.Count} 张图片"
            : "结果会显示在这里，不会进入聊天记录";

    public string ImageCanvasStatusLabel => IsGeneratingImage
        ? "正在生成"
        : LatestImageBatch is not null
            ? "已完成"
            : string.Empty;

    public bool HasImageCanvasStatus => IsGeneratingImage || LatestImageBatch is not null;

    public string VideoCanvasSubtitle => IsGeneratingVideo
        ? VideoProgressLabel
        : LatestVideo is { } latest
            ? $"{latest.ModelName} · 已保存到本机"
            : VideoProgress?.Status is "failed"
                ? "生成失败"
                : "视频结果不会进入聊天记录";

    public string VideoProgressLabel => VideoProgress switch
    {
        { Status: "downloading" } => "正在下载视频…",
        { Percent: { } percent } => $"{VideoProgress.Status} · {percent:0}%",
        { } value => value.Status,
        _ => "选择视频模型并描述镜头",
    };

    public string VideoFirstFrameLabel => VideoFirstFrame?.Name ?? "未选择";

    public bool HasVideoFirstFrame => VideoFirstFrame is not null;

    public bool CanChooseVideoRatio => VideoFirstFrame is null;

    public string VideoRatioHint => VideoFirstFrame is null
        ? "未选择首帧时使用此比例"
        : "已跟随首帧图片比例";

    public string VideoReferenceAudioLabel => VideoReferenceAudio?.Name ?? "未选择";

    public bool HasVideoReferenceAudio => VideoReferenceAudio is not null;

    public string VideoReferenceAudioHint => !CanUseVideoReferenceAudio
        ? "当前视频模型不支持参考音频"
        : HasVideoReferenceAudio
            ? "生成时将使用这段音频作为动作与节奏参考"
            : "支持 MP3、WAV、M4A 或 AAC，最大 20 MB";

    private VideoGenerationProfile VideoProfile =>
        VideoGenerationProfile.ForModel(SelectedVideoModel?.ModelName ?? string.Empty);

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerate))]
    private MediaGenerationModel? _selectedModel;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerate))]
    private string _prompt = string.Empty;

    [ObservableProperty]
    private string _selectedSize = "1024x1024";

    [ObservableProperty]
    private int _selectedCount = 1;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerateVideo))]
    private MediaGenerationModel? _selectedVideoModel;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerateVideo))]
    private string _videoPrompt = string.Empty;

    [ObservableProperty]
    private string _videoSize = "1280x720";

    [ObservableProperty]
    private int _videoSeconds = 4;

    [ObservableProperty]
    private string _videoRatio = "16:9";

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(VideoFirstFrameLabel))]
    [NotifyPropertyChangedFor(nameof(HasVideoFirstFrame))]
    [NotifyPropertyChangedFor(nameof(CanChooseVideoRatio))]
    [NotifyPropertyChangedFor(nameof(VideoRatioHint))]
    [NotifyPropertyChangedFor(nameof(GenerateVideoActionLabel))]
    private ImageGenerationInput? _videoFirstFrame;

    [ObservableProperty]
    private string? _videoFirstFramePreviewPath;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(VideoReferenceAudioLabel))]
    [NotifyPropertyChangedFor(nameof(HasVideoReferenceAudio))]
    [NotifyPropertyChangedFor(nameof(VideoReferenceAudioHint))]
    private VideoGenerationInputAudio? _videoReferenceAudio;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerateVideo))]
    [NotifyPropertyChangedFor(nameof(VideoCanvasSubtitle))]
    private bool _isGeneratingVideo;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(VideoProgressLabel))]
    [NotifyPropertyChangedFor(nameof(VideoCanvasSubtitle))]
    private VideoGenerationProgress? _videoProgress;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(ImageCanvasSubtitle))]
    [NotifyPropertyChangedFor(nameof(ImageCanvasStatusLabel))]
    [NotifyPropertyChangedFor(nameof(HasImageCanvasStatus))]
    private MediaStudioHistoryItem? _latestImageBatch;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(VideoCanvasSubtitle))]
    private MediaStudioVideoHistoryItem? _latestVideo;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerate))]
    [NotifyPropertyChangedFor(nameof(ImageCanvasSubtitle))]
    [NotifyPropertyChangedFor(nameof(ImageCanvasStatusLabel))]
    [NotifyPropertyChangedFor(nameof(HasImageCanvasStatus))]
    [NotifyPropertyChangedFor(nameof(GenerateImageActionLabel))]
    private bool _isGeneratingImage;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerate))]
    [NotifyPropertyChangedFor(nameof(CanAddReferenceImages))]
    [NotifyPropertyChangedFor(nameof(CanClearReferenceImages))]
    private bool _isBusy;

    [ObservableProperty]
    private bool _isInitialLoading;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(ShowImageModelLoading))]
    [NotifyPropertyChangedFor(nameof(ShowVideoModelLoading))]
    [NotifyPropertyChangedFor(nameof(ShowImageModelEmpty))]
    [NotifyPropertyChangedFor(nameof(ShowVideoModelEmpty))]
    private bool _isLoadingModels;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string _statusMessage = "选择模型并描述想要生成的画面";

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanLoadMoreHistory))]
    private bool _hasMoreHistory;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanLoadMoreVideoHistory))]
    private bool _hasMoreVideoHistory;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanLoadMoreHistory))]
    private bool _isLoadingMoreHistory;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanLoadMoreVideoHistory))]
    private bool _isLoadingMoreVideoHistory;

    public bool CanLoadMoreHistory => HasMoreHistory && !IsLoadingMoreHistory;

    public bool CanLoadMoreVideoHistory => HasMoreVideoHistory && !IsLoadingMoreVideoHistory;

    public async Task OpenAsync(string ownerUserId, CancellationToken cancellationToken = default)
    {
        if (!string.Equals(_ownerUserId, ownerUserId, StringComparison.Ordinal))
        {
            Reset();
            _ownerUserId = ownerUserId;
        }
        if (_initialized) return;
        var session = _session;
        _initialized = true;
        IsInitialLoading = true;
        IsLoadingModels = true;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            var historyTask = _historyStore.LoadPageAsync(
                ownerUserId, null, cancellationToken: cancellationToken);
            var videoHistoryTask = _historyStore.LoadVideoPageAsync(
                ownerUserId, null, cancellationToken: cancellationToken);
            var modelsTask = _service.FetchModelsAsync(cancellationToken);
            await Task.WhenAll(historyTask, videoHistoryTask, modelsTask);
            if (session != _session || _ownerUserId != ownerUserId) return;
            foreach (var item in historyTask.Result.Items) History.Add(item);
            foreach (var item in videoHistoryTask.Result.Items) VideoHistory.Add(item);
            _historyCursor = historyTask.Result.NextCursor;
            _videoHistoryCursor = videoHistoryTask.Result.NextCursor;
            HasMoreHistory = _historyCursor is not null;
            HasMoreVideoHistory = _videoHistoryCursor is not null;
            ShowLatest(History.FirstOrDefault());
            LatestVideo = VideoHistory.FirstOrDefault();
            ApplyModels(modelsTask.Result);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            if (session == _session)
            {
                _initialized = false;
                ErrorMessage = exception.Message;
            }
        }
        finally
        {
            if (session == _session)
            {
                IsInitialLoading = false;
                IsLoadingModels = false;
                IsBusy = false;
            }
        }
    }

    public async Task LoadMoreHistoryAsync(CancellationToken cancellationToken = default)
    {
        var owner = _ownerUserId;
        var cursor = _historyCursor;
        if (owner is null || cursor is null || IsLoadingMoreHistory) return;
        var session = _session;
        IsLoadingMoreHistory = true;
        try
        {
            var page = await _historyStore.LoadPageAsync(
                owner, cursor, cancellationToken: cancellationToken);
            if (session != _session || _ownerUserId != owner) return;
            foreach (var item in page.Items) History.Add(item);
            _historyCursor = page.NextCursor;
            HasMoreHistory = _historyCursor is not null;
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            if (session == _session) ErrorMessage = exception.Message;
        }
        finally
        {
            if (session == _session) IsLoadingMoreHistory = false;
        }
    }

    public async Task LoadMoreVideoHistoryAsync(CancellationToken cancellationToken = default)
    {
        var owner = _ownerUserId;
        var cursor = _videoHistoryCursor;
        if (owner is null || cursor is null || IsLoadingMoreVideoHistory) return;
        var session = _session;
        IsLoadingMoreVideoHistory = true;
        try
        {
            var page = await _historyStore.LoadVideoPageAsync(
                owner, cursor, cancellationToken: cancellationToken);
            if (session != _session || _ownerUserId != owner) return;
            foreach (var item in page.Items) VideoHistory.Add(item);
            _videoHistoryCursor = page.NextCursor;
            HasMoreVideoHistory = _videoHistoryCursor is not null;
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            if (session == _session) ErrorMessage = exception.Message;
        }
        finally
        {
            if (session == _session) IsLoadingMoreVideoHistory = false;
        }
    }

    public async Task ReloadModelsAsync(CancellationToken cancellationToken = default)
    {
        IsLoadingModels = true;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            ApplyModels(await _service.FetchModelsAsync(cancellationToken));
            StatusMessage = Models.Count == 0
                ? "没有找到已配置的图片模型"
                : $"已加载 {Models.Count} 个可用模型";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
        finally
        {
            IsLoadingModels = false;
            IsBusy = false;
        }
    }

    public async Task GenerateAsync(CancellationToken cancellationToken = default)
    {
        var owner = _ownerUserId;
        if (!CanGenerate || SelectedModel is null || owner is null) return;
        var submittedPrompt = Prompt.Trim();
        IsGeneratingImage = true;
        IsBusy = true;
        ErrorMessage = null;
        StatusMessage = "正在生成图片…";
        try
        {
            var result = await _service.GenerateImageAsync(
                new ImageGenerationRequest(
                    SelectedModel.Id,
                    submittedPrompt,
                    SelectedSize == "auto" ? null : SelectedSize,
                    SelectedCount,
                    ReferenceImages.Select(image => image.Input).ToArray()),
                cancellationToken);
            var item = await _historyStore.SaveAsync(
                owner,
                submittedPrompt,
                result,
                cancellationToken);
            History.Insert(0, item);
            ShowLatest(item);
            StatusMessage = $"已生成并保存 {item.Images.Count} 张图片";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
            StatusMessage = "生成失败";
        }
        finally
        {
            IsGeneratingImage = false;
            IsBusy = false;
        }
    }

    public async Task AddReferenceImagesAsync(
        IReadOnlyList<string> paths,
        CancellationToken cancellationToken = default)
    {
        ErrorMessage = null;
        if (ReferenceImages.Count + paths.Count > 8)
        {
            ErrorMessage = "最多可以添加 8 张参考图。";
            return;
        }
        try
        {
            foreach (var path in paths)
            {
                var mimeType = Path.GetExtension(path).ToLowerInvariant() switch
                {
                    ".png" => "image/png",
                    ".jpg" or ".jpeg" => "image/jpeg",
                    ".webp" => "image/webp",
                    _ => throw new InvalidDataException("参考图必须是 PNG、JPEG 或 WebP。"),
                };
                var bytes = await File.ReadAllBytesAsync(path, cancellationToken);
                if (bytes.Length == 0 || bytes.Length > MaximumImageBytes)
                    throw new InvalidDataException("参考图为空或超过 20 MB。");
                ReferenceImages.Add(new MediaStudioReferenceImage(
                    new ImageGenerationInput(
                        Path.GetFileName(path),
                        mimeType,
                        Convert.ToBase64String(bytes)),
                    path));
            }
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
    }

    public void RemoveReferenceImage(MediaStudioReferenceImage image) => ReferenceImages.Remove(image);

    public void ClearReferenceImages() => ReferenceImages.Clear();

    public async Task SetVideoFirstFrameAsync(
        string path,
        CancellationToken cancellationToken = default)
    {
        ErrorMessage = null;
        try
        {
            var input = await LoadImageInputAsync(path, cancellationToken);
            VideoFirstFramePreviewPath = path;
            VideoFirstFrame = input;
            VideoReferenceAudio = null;
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
    }

    public void RemoveVideoFirstFrame()
    {
        VideoFirstFrame = null;
        VideoFirstFramePreviewPath = null;
    }

    public async Task SetVideoReferenceAudioAsync(
        string path,
        CancellationToken cancellationToken = default)
    {
        ErrorMessage = null;
        try
        {
            var mimeType = Path.GetExtension(path).ToLowerInvariant() switch
            {
                ".mp3" => "audio/mpeg",
                ".wav" => "audio/wav",
                ".m4a" => "audio/mp4",
                ".aac" => "audio/aac",
                _ => throw new InvalidDataException("参考音频必须是 MP3、WAV、M4A 或 AAC。"),
            };
            var bytes = await File.ReadAllBytesAsync(path, cancellationToken);
            if (bytes.Length == 0 || bytes.Length > MaximumImageBytes)
                throw new InvalidDataException("参考音频为空或超过 20 MB。");
            VideoReferenceAudio = new VideoGenerationInputAudio(
                Path.GetFileName(path),
                mimeType,
                Convert.ToBase64String(bytes));
            VideoFirstFrame = null;
            VideoFirstFramePreviewPath = null;
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
    }

    public void RemoveVideoReferenceAudio() => VideoReferenceAudio = null;

    public async Task GenerateVideoAsync(CancellationToken cancellationToken = default)
    {
        var owner = _ownerUserId;
        if (!CanGenerateVideo || SelectedVideoModel is null || owner is null) return;
        _videoGenerationCancellation?.Cancel();
        _videoGenerationCancellation?.Dispose();
        _videoGenerationCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var token = _videoGenerationCancellation.Token;
        var submittedPrompt = VideoPrompt.Trim();
        IsGeneratingVideo = true;
        ErrorMessage = null;
        VideoProgress = new VideoGenerationProgress("submitting");
        try
        {
            var progress = new Progress<VideoGenerationProgress>(value => VideoProgress = value);
            var result = await _service.GenerateVideoAsync(
                new VideoGenerationRequest(
                    SelectedVideoModel.Id,
                    submittedPrompt,
                    VideoSize,
                    VideoSeconds,
                    VideoFirstFrame,
                    null,
                    VideoReferenceAudio,
                    VideoRatio),
                progress,
                token);
            VideoProgress = new VideoGenerationProgress("saving", 100, result.Id);
            var item = await _historyStore.SaveVideoAsync(
                owner,
                submittedPrompt,
                result,
                token);
            VideoHistory.Insert(0, item);
            LatestVideo = item;
            VideoProgress = new VideoGenerationProgress("completed", 100, result.Id);
            StatusMessage = "视频已生成并保存到本机";
        }
        catch (OperationCanceledException)
        {
            VideoProgress = null;
            StatusMessage = "已停止等待视频任务";
        }
        catch (Exception exception)
        {
            ErrorMessage = exception.Message;
            VideoProgress = new VideoGenerationProgress("failed");
        }
        finally
        {
            IsGeneratingVideo = false;
        }
    }

    public void CancelVideoGeneration() => _videoGenerationCancellation?.Cancel();

    public void SelectHistoryItem(MediaStudioHistoryItem item) => ShowLatest(item);

    public void SelectVideoHistoryItem(MediaStudioVideoHistoryItem item) => LatestVideo = item;

    public void ReportError(string message) => ErrorMessage = message;

    public void Reset()
    {
        _session = Guid.NewGuid();
        _videoGenerationCancellation?.Cancel();
        _videoGenerationCancellation?.Dispose();
        _videoGenerationCancellation = null;
        _ownerUserId = null;
        _initialized = false;
        _historyCursor = null;
        _videoHistoryCursor = null;
        HasMoreHistory = false;
        HasMoreVideoHistory = false;
        IsInitialLoading = false;
        IsLoadingModels = false;
        IsGeneratingImage = false;
        IsLoadingMoreHistory = false;
        IsLoadingMoreVideoHistory = false;
        Models.Clear();
        VideoModels.Clear();
        History.Clear();
        VideoHistory.Clear();
        LatestImages.Clear();
        LatestImageBatch = null;
        ReferenceImages.Clear();
        SelectedModel = null;
        SelectedVideoModel = null;
        LatestVideo = null;
        VideoFirstFrame = null;
        VideoFirstFramePreviewPath = null;
        VideoReferenceAudio = null;
        VideoProgress = null;
        Prompt = string.Empty;
        VideoPrompt = string.Empty;
        ErrorMessage = null;
        StatusMessage = "选择模型并描述想要生成的画面";
    }

    private void ApplyModels(IReadOnlyList<MediaGenerationModel> values)
    {
        var selectedId = SelectedModel?.Id;
        Models.Clear();
        foreach (var model in values.Where(model => !model.IsLikelyVideoModel)) Models.Add(model);
        SelectedModel = Models.FirstOrDefault(model => model.Id == selectedId) ?? Models.FirstOrDefault();
        var selectedVideoId = SelectedVideoModel?.Id;
        VideoModels.Clear();
        foreach (var model in values.Where(model => model.IsLikelyVideoModel))
            VideoModels.Add(model);
        SelectedVideoModel = VideoModels.FirstOrDefault(model => model.Id == selectedVideoId) ??
            VideoModels.FirstOrDefault();
        if (Models.Count == 0) StatusMessage = "没有找到已配置的图片模型";
    }

    private void ShowLatest(MediaStudioHistoryItem? item)
    {
        LatestImageBatch = item;
        LatestImages.Clear();
        if (item is null) return;
        foreach (var image in item.Images) LatestImages.Add(image);
    }

    partial void OnSelectedVideoModelChanged(MediaGenerationModel? value)
    {
        var profile = VideoProfile;
        if (!profile.Sizes.Contains(VideoSize)) VideoSize = profile.Sizes[0];
        if (!profile.Durations.Contains(VideoSeconds)) VideoSeconds = profile.Durations[0];
        if (!profile.SupportsReferenceVideo) VideoReferenceAudio = null;
        OnPropertyChanged(nameof(VideoSizes));
        OnPropertyChanged(nameof(VideoDurations));
        OnPropertyChanged(nameof(CanUseVideoReferenceAudio));
        OnPropertyChanged(nameof(VideoReferenceAudioHint));
    }

    private static async Task<ImageGenerationInput> LoadImageInputAsync(
        string path,
        CancellationToken cancellationToken)
    {
        var mimeType = Path.GetExtension(path).ToLowerInvariant() switch
        {
            ".png" => "image/png",
            ".jpg" or ".jpeg" => "image/jpeg",
            ".webp" => "image/webp",
            _ => throw new InvalidDataException("参考图必须是 PNG、JPEG 或 WebP。"),
        };
        var bytes = await File.ReadAllBytesAsync(path, cancellationToken);
        if (bytes.Length == 0 || bytes.Length > MaximumImageBytes)
            throw new InvalidDataException("参考图为空或超过 20 MB。");
        return new ImageGenerationInput(
            Path.GetFileName(path),
            mimeType,
            Convert.ToBase64String(bytes));
    }
}

public sealed record MediaStudioReferenceImage(ImageGenerationInput Input, string FilePath)
{
    public string Name => Input.Name;
}
