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
    private CancellationTokenSource? _imageGenerationCancellation;
    private CancellationTokenSource? _modelReloadCancellation;
    private CancellationTokenSource? _referenceImagesCancellation;
    private CancellationTokenSource? _videoFirstFrameCancellation;
    private CancellationTokenSource? _videoReferenceAudioCancellation;
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

    public bool CanUseVideoReferenceAudio =>
        VideoProfile.SupportsReferenceVideo && !IsLoadingVideoReferenceAudio && !IsGeneratingVideo;

    public bool CanChooseVideoFirstFrame => !IsLoadingVideoFirstFrame && !IsGeneratingVideo;

    public bool ShowVideoRatio
    {
        get
        {
            var model = SelectedVideoModel?.ModelName ?? string.Empty;
            return model.Contains("minimax", StringComparison.OrdinalIgnoreCase) ||
                model.Contains("seedance", StringComparison.OrdinalIgnoreCase);
        }
    }

    public string VideoSizeLabel => ShowVideoRatio ? "分辨率" : "画幅";

    public bool CanGenerate => !IsBusy && !IsLoadingReferenceImages && SelectedModel is not null &&
        !string.IsNullOrWhiteSpace(Prompt) && _ownerUserId is not null;

    public bool CanReloadModels => !IsBusy;

    public bool CanGenerateVideo => !IsGeneratingVideo && !IsLoadingVideoFirstFrame &&
        !IsLoadingVideoReferenceAudio && SelectedVideoModel is not null &&
        !string.IsNullOrWhiteSpace(VideoPrompt) && _ownerUserId is not null;

    public bool HasImageModels => Models.Count > 0;

    public bool HasVideoModels => VideoModels.Count > 0;

    public bool ShowImageModelLoading => IsLoadingModels && !HasImageModels;

    public bool ShowVideoModelLoading => IsLoadingModels && !HasVideoModels;

    public bool ShowImageModelEmpty => !IsLoadingModels && !HasImageModels;

    public bool ShowVideoModelEmpty => !IsLoadingModels && !HasVideoModels;

    public bool HasReferenceImages => ReferenceImages.Count > 0;

    public bool CanAddReferenceImages => !IsBusy && !IsLoadingReferenceImages && ReferenceImages.Count < 8;

    public bool CanClearReferenceImages => !IsBusy && !IsLoadingReferenceImages && HasReferenceImages;

    public string ReferenceImageCountLabel => $"{ReferenceImages.Count} / 8";

    public string GenerateImageActionLabel => IsGeneratingImage
        ? "正在生成…"
        : HasReferenceImages
            ? "基于参考图生成"
            : "生成图片";

    public string ImageEmptyDescription => HasReferenceImages
        ? "参考图已经添加。请在右侧描述希望保留、修改或重新设计的内容。"
        : "从一个清晰的主体、场景或风格开始，也可以添加参考图进行图生图。";

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

    public bool HasCompletedImageCanvasStatus => !IsGeneratingImage && LatestImageBatch is not null;

    public string VideoCanvasSubtitle => IsGeneratingVideo
        ? VideoProgressLabel
        : LatestVideo is { } latest
            ? $"{latest.ModelName} · 已保存到本机"
            : VideoProgress?.Status is "failed"
                ? "生成失败"
                : "视频结果不会进入聊天记录";

    public string VideoProgressLabel => VideoProgress switch
    {
        { Percent: { } percent } value => $"{LocalizeVideoStatus(value.Status)} · {percent:0}%",
        { } value => LocalizeVideoStatus(value.Status),
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
    [NotifyPropertyChangedFor(nameof(CanChooseVideoFirstFrame))]
    [NotifyPropertyChangedFor(nameof(CanGenerateVideo))]
    private bool _isLoadingVideoFirstFrame;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanUseVideoReferenceAudio))]
    [NotifyPropertyChangedFor(nameof(CanGenerateVideo))]
    private bool _isLoadingVideoReferenceAudio;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerateVideo))]
    [NotifyPropertyChangedFor(nameof(VideoCanvasSubtitle))]
    [NotifyPropertyChangedFor(nameof(CanChooseVideoFirstFrame))]
    [NotifyPropertyChangedFor(nameof(CanUseVideoReferenceAudio))]
    private bool _isGeneratingVideo;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(VideoProgressLabel))]
    [NotifyPropertyChangedFor(nameof(VideoCanvasSubtitle))]
    private VideoGenerationProgress? _videoProgress;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(ImageCanvasSubtitle))]
    [NotifyPropertyChangedFor(nameof(ImageCanvasStatusLabel))]
    [NotifyPropertyChangedFor(nameof(HasImageCanvasStatus))]
    [NotifyPropertyChangedFor(nameof(HasCompletedImageCanvasStatus))]
    private MediaStudioHistoryItem? _latestImageBatch;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(VideoCanvasSubtitle))]
    private MediaStudioVideoHistoryItem? _latestVideo;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerate))]
    [NotifyPropertyChangedFor(nameof(ImageCanvasSubtitle))]
    [NotifyPropertyChangedFor(nameof(ImageCanvasStatusLabel))]
    [NotifyPropertyChangedFor(nameof(HasImageCanvasStatus))]
    [NotifyPropertyChangedFor(nameof(HasCompletedImageCanvasStatus))]
    [NotifyPropertyChangedFor(nameof(GenerateImageActionLabel))]
    private bool _isGeneratingImage;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerate))]
    [NotifyPropertyChangedFor(nameof(CanAddReferenceImages))]
    [NotifyPropertyChangedFor(nameof(CanClearReferenceImages))]
    [NotifyPropertyChangedFor(nameof(CanReloadModels))]
    private bool _isBusy;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerate))]
    [NotifyPropertyChangedFor(nameof(CanAddReferenceImages))]
    [NotifyPropertyChangedFor(nameof(CanClearReferenceImages))]
    private bool _isLoadingReferenceImages;

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
            StartupDiagnostics.RecordUnhandled("media studio workspace data loading", exception);
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
        if (!CanReloadModels) return;
        _modelReloadCancellation?.Cancel();
        _modelReloadCancellation?.Dispose();
        var operation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        _modelReloadCancellation = operation;
        var token = operation.Token;
        var session = _session;
        var owner = _ownerUserId;
        IsLoadingModels = true;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            var models = await _service.FetchModelsAsync(token);
            token.ThrowIfCancellationRequested();
            if (session != _session || !string.Equals(owner, _ownerUserId, StringComparison.Ordinal)) return;
            ApplyModels(models);
            StatusMessage = Models.Count == 0
                ? "没有找到已配置的图片模型"
                : $"已加载 {Models.Count} 个可用模型";
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            if (session == _session) ErrorMessage = exception.Message;
        }
        finally
        {
            if (session == _session && ReferenceEquals(_modelReloadCancellation, operation))
            {
                IsLoadingModels = false;
                IsBusy = false;
                _modelReloadCancellation.Dispose();
                _modelReloadCancellation = null;
            }
        }
    }

    public async Task GenerateAsync(CancellationToken cancellationToken = default)
    {
        var owner = _ownerUserId;
        if (!CanGenerate || SelectedModel is null || owner is null) return;
        _imageGenerationCancellation?.Cancel();
        _imageGenerationCancellation?.Dispose();
        var operation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        _imageGenerationCancellation = operation;
        var token = operation.Token;
        var session = _session;
        var submittedPrompt = Prompt.Trim();
        var generationCompleted = false;
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
                token);
            token.ThrowIfCancellationRequested();
            generationCompleted = true;
            if (session != _session || !string.Equals(owner, _ownerUserId, StringComparison.Ordinal)) return;
            var item = await _historyStore.SaveAsync(
                owner,
                submittedPrompt,
                result,
                token);
            token.ThrowIfCancellationRequested();
            if (session != _session || !string.Equals(owner, _ownerUserId, StringComparison.Ordinal)) return;
            History.Insert(0, item);
            ShowLatest(item);
            StatusMessage = $"已生成并保存 {item.Images.Count} 张图片";
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            if (session == _session)
            {
                ErrorMessage = generationCompleted
                    ? $"图片已生成，但未能保存到本机：{exception.Message}"
                    : exception.Message;
                StatusMessage = generationCompleted ? "图片已生成，但保存失败" : "生成失败";
            }
        }
        finally
        {
            if (session == _session && ReferenceEquals(_imageGenerationCancellation, operation))
            {
                IsGeneratingImage = false;
                IsBusy = false;
                _imageGenerationCancellation.Dispose();
                _imageGenerationCancellation = null;
            }
        }
    }

    public async Task AddReferenceImagesAsync(
        IReadOnlyList<string> paths,
        CancellationToken cancellationToken = default)
    {
        ErrorMessage = null;
        if (paths.Count == 0) return;
        if (ReferenceImages.Count + paths.Count > 8)
        {
            ErrorMessage = "最多可以添加 8 张参考图。";
            return;
        }
        _referenceImagesCancellation?.Cancel();
        _referenceImagesCancellation?.Dispose();
        var operation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        _referenceImagesCancellation = operation;
        var token = operation.Token;
        var session = _session;
        var owner = _ownerUserId;
        IsLoadingReferenceImages = true;
        try
        {
            var loaded = new List<MediaStudioReferenceImage>(paths.Count);
            foreach (var path in paths)
            {
                var mimeType = Path.GetExtension(path).ToLowerInvariant() switch
                {
                    ".png" => "image/png",
                    ".jpg" or ".jpeg" => "image/jpeg",
                    ".webp" => "image/webp",
                    _ => throw new InvalidDataException("参考图必须是 PNG、JPEG 或 WebP。"),
                };
                var bytes = await File.ReadAllBytesAsync(path, token);
                if (bytes.Length == 0 || bytes.Length > MaximumImageBytes)
                    throw new InvalidDataException("参考图为空或超过 20 MB。");
                loaded.Add(new MediaStudioReferenceImage(
                    new ImageGenerationInput(
                        Path.GetFileName(path),
                        mimeType,
                        Convert.ToBase64String(bytes)),
                    path));
            }
            token.ThrowIfCancellationRequested();
            if (session != _session || !string.Equals(owner, _ownerUserId, StringComparison.Ordinal)) return;
            foreach (var image in loaded) ReferenceImages.Add(image);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            if (session == _session) ErrorMessage = exception.Message;
        }
        finally
        {
            if (session == _session && ReferenceEquals(_referenceImagesCancellation, operation))
            {
                IsLoadingReferenceImages = false;
                _referenceImagesCancellation.Dispose();
                _referenceImagesCancellation = null;
            }
        }
    }

    public void RemoveReferenceImage(MediaStudioReferenceImage image) => ReferenceImages.Remove(image);

    public void ClearReferenceImages() => ReferenceImages.Clear();

    public async Task SetVideoFirstFrameAsync(
        string path,
        CancellationToken cancellationToken = default)
    {
        _videoFirstFrameCancellation?.Cancel();
        _videoFirstFrameCancellation?.Dispose();
        var operation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        _videoFirstFrameCancellation = operation;
        var token = operation.Token;
        var session = _session;
        var owner = _ownerUserId;
        ErrorMessage = null;
        IsLoadingVideoFirstFrame = true;
        try
        {
            var input = await LoadImageInputAsync(path, token);
            token.ThrowIfCancellationRequested();
            if (session != _session || !string.Equals(owner, _ownerUserId, StringComparison.Ordinal)) return;
            VideoFirstFramePreviewPath = path;
            VideoFirstFrame = input;
            VideoReferenceAudio = null;
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            if (session == _session) ErrorMessage = exception.Message;
        }
        finally
        {
            if (session == _session && ReferenceEquals(_videoFirstFrameCancellation, operation))
            {
                IsLoadingVideoFirstFrame = false;
                _videoFirstFrameCancellation.Dispose();
                _videoFirstFrameCancellation = null;
            }
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
        _videoReferenceAudioCancellation?.Cancel();
        _videoReferenceAudioCancellation?.Dispose();
        var operation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        _videoReferenceAudioCancellation = operation;
        var token = operation.Token;
        var session = _session;
        var owner = _ownerUserId;
        ErrorMessage = null;
        IsLoadingVideoReferenceAudio = true;
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
            var bytes = await File.ReadAllBytesAsync(path, token);
            if (bytes.Length == 0 || bytes.Length > MaximumImageBytes)
                throw new InvalidDataException("参考音频为空或超过 20 MB。");
            token.ThrowIfCancellationRequested();
            if (session != _session || !string.Equals(owner, _ownerUserId, StringComparison.Ordinal)) return;
            VideoReferenceAudio = new VideoGenerationInputAudio(
                Path.GetFileName(path),
                mimeType,
                Convert.ToBase64String(bytes));
            VideoFirstFrame = null;
            VideoFirstFramePreviewPath = null;
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            if (session == _session) ErrorMessage = exception.Message;
        }
        finally
        {
            if (session == _session && ReferenceEquals(_videoReferenceAudioCancellation, operation))
            {
                IsLoadingVideoReferenceAudio = false;
                _videoReferenceAudioCancellation.Dispose();
                _videoReferenceAudioCancellation = null;
            }
        }
    }

    public void RemoveVideoReferenceAudio() => VideoReferenceAudio = null;

    public async Task GenerateVideoAsync(CancellationToken cancellationToken = default)
    {
        var owner = _ownerUserId;
        if (!CanGenerateVideo || SelectedVideoModel is null || owner is null) return;
        _videoGenerationCancellation?.Cancel();
        _videoGenerationCancellation?.Dispose();
        var operation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        _videoGenerationCancellation = operation;
        var token = operation.Token;
        var session = _session;
        var submittedPrompt = VideoPrompt.Trim();
        var generationCompleted = false;
        IsGeneratingVideo = true;
        ErrorMessage = null;
        VideoProgress = new VideoGenerationProgress("submitting");
        try
        {
            var progress = new Progress<VideoGenerationProgress>(value =>
            {
                if (!token.IsCancellationRequested && session == _session &&
                    string.Equals(owner, _ownerUserId, StringComparison.Ordinal) &&
                    ReferenceEquals(_videoGenerationCancellation, operation))
                    VideoProgress = value;
            });
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
            token.ThrowIfCancellationRequested();
            generationCompleted = true;
            if (session != _session || !string.Equals(owner, _ownerUserId, StringComparison.Ordinal) ||
                !ReferenceEquals(_videoGenerationCancellation, operation)) return;
            VideoProgress = new VideoGenerationProgress("saving", 100, result.Id);
            var item = await _historyStore.SaveVideoAsync(
                owner,
                submittedPrompt,
                result,
                token);
            token.ThrowIfCancellationRequested();
            if (session != _session || !string.Equals(owner, _ownerUserId, StringComparison.Ordinal) ||
                !ReferenceEquals(_videoGenerationCancellation, operation)) return;
            VideoHistory.Insert(0, item);
            LatestVideo = item;
            VideoProgress = new VideoGenerationProgress("completed", 100, result.Id);
            StatusMessage = "视频已生成并保存到本机";
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
            if (session == _session && ReferenceEquals(_videoGenerationCancellation, operation))
            {
                VideoProgress = null;
                StatusMessage = "已停止等待视频任务";
            }
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            if (session == _session && ReferenceEquals(_videoGenerationCancellation, operation))
            {
                ErrorMessage = generationCompleted
                    ? $"视频已生成，但未能保存到本机：{exception.Message}"
                    : exception.Message;
                VideoProgress = new VideoGenerationProgress("failed");
            }
        }
        finally
        {
            if (session == _session && ReferenceEquals(_videoGenerationCancellation, operation))
            {
                IsGeneratingVideo = false;
                _videoGenerationCancellation.Dispose();
                _videoGenerationCancellation = null;
            }
        }
    }

    public void CancelVideoGeneration() => _videoGenerationCancellation?.Cancel();

    public void SelectHistoryItem(MediaStudioHistoryItem item) => ShowLatest(item);

    public void SelectVideoHistoryItem(MediaStudioVideoHistoryItem item) => LatestVideo = item;

    public void ReportError(string message) => ErrorMessage = message;

    public void Reset()
    {
        _session = Guid.NewGuid();
        _imageGenerationCancellation?.Cancel();
        _imageGenerationCancellation?.Dispose();
        _imageGenerationCancellation = null;
        _modelReloadCancellation?.Cancel();
        _modelReloadCancellation?.Dispose();
        _modelReloadCancellation = null;
        _referenceImagesCancellation?.Cancel();
        _referenceImagesCancellation?.Dispose();
        _referenceImagesCancellation = null;
        _videoFirstFrameCancellation?.Cancel();
        _videoFirstFrameCancellation?.Dispose();
        _videoFirstFrameCancellation = null;
        _videoReferenceAudioCancellation?.Cancel();
        _videoReferenceAudioCancellation?.Dispose();
        _videoReferenceAudioCancellation = null;
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
        IsGeneratingVideo = false;
        IsLoadingReferenceImages = false;
        IsLoadingVideoFirstFrame = false;
        IsLoadingVideoReferenceAudio = false;
        IsBusy = false;
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
        // macOS keeps every enabled model with credentials in the image creation
        // picker. A model's task flag (or its name looking video-like) does not
        // make it unavailable for an independent creation request.
        foreach (var model in values) Models.Add(model);
        SelectedModel = Models.FirstOrDefault(model => model.Id == selectedId) ?? Models.FirstOrDefault();
        var selectedVideoId = SelectedVideoModel?.Id;
        VideoModels.Clear();
        // Keep the video picker in lockstep with macOS: the current unified
        // video transport is offered only for MiniMax-H3.
        foreach (var model in values.Where(model =>
                     string.Equals(model.ModelName.Trim(), "MiniMax-H3", StringComparison.OrdinalIgnoreCase)))
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
        OnPropertyChanged(nameof(VideoSize));
        OnPropertyChanged(nameof(VideoSeconds));
        OnPropertyChanged(nameof(CanUseVideoReferenceAudio));
        OnPropertyChanged(nameof(VideoReferenceAudioHint));
        OnPropertyChanged(nameof(ShowVideoRatio));
        OnPropertyChanged(nameof(VideoSizeLabel));
    }

    private static string LocalizeVideoStatus(string status) => status switch
    {
        "submitting" => "提交中",
        "uploading" => "上传素材中",
        "queued" => "排队中",
        "in_progress" => "生成中",
        "unknown" => "等待状态更新",
        "downloading" => "下载中",
        "saving" => "保存中",
        "failed" => "生成失败",
        "completed" => "已完成",
        _ => status,
    };

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
