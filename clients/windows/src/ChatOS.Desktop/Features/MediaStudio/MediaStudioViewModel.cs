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

    public MediaStudioViewModel(
        IMediaGenerationService service,
        MediaStudioHistoryStore historyStore)
    {
        _service = service;
        _historyStore = historyStore;
    }

    public ObservableCollection<MediaGenerationModel> Models { get; } = [];

    public ObservableCollection<MediaGenerationModel> VideoModels { get; } = [];

    public ObservableCollection<MediaStudioHistoryItem> History { get; } = [];

    public ObservableCollection<MediaStudioVideoHistoryItem> VideoHistory { get; } = [];

    public ObservableCollection<MediaStudioImageItem> LatestImages { get; } = [];

    public ObservableCollection<ImageGenerationInput> ReferenceImages { get; } = [];

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

    public string VideoProgressLabel => VideoProgress switch
    {
        { Status: "downloading" } => "正在下载视频…",
        { Percent: { } percent } => $"{VideoProgress.Status} · {percent:0}%",
        { } value => value.Status,
        _ => "选择视频模型并描述镜头",
    };

    public string VideoFirstFrameLabel => VideoFirstFrame?.Name ?? "未选择";

    public string VideoReferenceAudioLabel => VideoReferenceAudio?.Name ?? "未选择";

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
    private ImageGenerationInput? _videoFirstFrame;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(VideoReferenceAudioLabel))]
    private VideoGenerationInputAudio? _videoReferenceAudio;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerateVideo))]
    private bool _isGeneratingVideo;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(VideoProgressLabel))]
    private VideoGenerationProgress? _videoProgress;

    [ObservableProperty]
    private MediaStudioVideoHistoryItem? _latestVideo;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerate))]
    private bool _isBusy;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string _statusMessage = "选择模型并描述想要生成的画面";

    public async Task OpenAsync(string ownerUserId, CancellationToken cancellationToken = default)
    {
        if (!string.Equals(_ownerUserId, ownerUserId, StringComparison.Ordinal))
        {
            Reset();
            _ownerUserId = ownerUserId;
        }
        if (_initialized) return;
        _initialized = true;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            var historyTask = _historyStore.LoadAsync(ownerUserId, cancellationToken);
            var videoHistoryTask = _historyStore.LoadVideosAsync(ownerUserId, cancellationToken);
            var modelsTask = _service.FetchModelsAsync(cancellationToken);
            await Task.WhenAll(historyTask, videoHistoryTask, modelsTask);
            foreach (var item in historyTask.Result) History.Add(item);
            foreach (var item in videoHistoryTask.Result) VideoHistory.Add(item);
            ShowLatest(History.FirstOrDefault());
            LatestVideo = VideoHistory.FirstOrDefault();
            ApplyModels(modelsTask.Result);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            _initialized = false;
            ErrorMessage = exception.Message;
        }
        finally
        {
            IsBusy = false;
        }
    }

    public async Task ReloadModelsAsync(CancellationToken cancellationToken = default)
    {
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
            IsBusy = false;
        }
    }

    public async Task GenerateAsync(CancellationToken cancellationToken = default)
    {
        var owner = _ownerUserId;
        if (!CanGenerate || SelectedModel is null || owner is null) return;
        var submittedPrompt = Prompt.Trim();
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
                    ReferenceImages.ToArray()),
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
                ReferenceImages.Add(new ImageGenerationInput(
                    Path.GetFileName(path),
                    mimeType,
                    Convert.ToBase64String(bytes)));
            }
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
    }

    public void RemoveReferenceImage(ImageGenerationInput image) => ReferenceImages.Remove(image);

    public async Task SetVideoFirstFrameAsync(
        string path,
        CancellationToken cancellationToken = default)
    {
        ErrorMessage = null;
        try
        {
            VideoFirstFrame = await LoadImageInputAsync(path, cancellationToken);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
    }

    public void RemoveVideoFirstFrame() => VideoFirstFrame = null;

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
        _videoGenerationCancellation?.Cancel();
        _videoGenerationCancellation?.Dispose();
        _videoGenerationCancellation = null;
        _ownerUserId = null;
        _initialized = false;
        Models.Clear();
        VideoModels.Clear();
        History.Clear();
        VideoHistory.Clear();
        LatestImages.Clear();
        ReferenceImages.Clear();
        SelectedModel = null;
        SelectedVideoModel = null;
        LatestVideo = null;
        VideoFirstFrame = null;
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
