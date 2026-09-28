using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class StorySegmentEditor : ObservableObject
{
    public StorySegmentEditor(StorySegmentDocument document, Func<string?, string?> resolvePath)
    {
        Id = document.Id;
        _title = document.Title;
        _narrative = document.Narrative;
        _imagePrompt = document.ImagePrompt;
        _videoPrompt = document.VideoPrompt;
        _seconds = document.Seconds;
        _firstFrameAsset = document.FirstFrameAsset;
        _lastFrameAsset = document.LastFrameAsset;
        _videoAsset = document.VideoAsset;
        _resourceIdsText = string.Join(", ", document.ResourceIds);
        _kind = document.Kind;
        _isRefined = document.IsRefined;
        _pendingVideoJobId = document.PendingVideoJobId;
        _pendingVideoJobStatus = document.PendingVideoJobStatus;
        _pendingVideoRequestDigest = document.PendingVideoRequestDigest;
        _continuityIn = document.ContinuityIn ?? string.Empty;
        _continuityOut = document.ContinuityOut ?? string.Empty;
        _shotPlan = document.ShotPlan ?? string.Empty;
        FirstFramePath = resolvePath(document.FirstFrameAsset);
        LastFramePath = resolvePath(document.LastFrameAsset);
        VideoPath = resolvePath(document.VideoAsset);
    }

    public string Id { get; }
    public string NumberLabel { get; internal set; } = string.Empty;
    public string? FirstFramePath { get; private set; }
    public string? LastFramePath { get; private set; }
    public string? VideoPath { get; private set; }
    public string FrameStatus => (FirstFramePath, LastFramePath) switch
    {
        ({ Length: > 0 }, { Length: > 0 }) => "首尾帧已就绪",
        ({ Length: > 0 }, _) => "首帧已就绪",
        _ => "尚未生成画面",
    };
    public string VideoStatus => VideoPath is { Length: > 0 }
        ? "视频已完成"
        : HasPendingVideoJob ? $"已有视频任务 · {PendingVideoJobStatus ?? "等待查询"}" : "视频待生成";
    public bool HasPendingVideoJob => !string.IsNullOrWhiteSpace(_pendingVideoJobId);
    public string? PendingVideoJobId => _pendingVideoJobId;
    public string? PendingVideoJobStatus => _pendingVideoJobStatus;
    public string? PendingVideoRequestDigest => _pendingVideoRequestDigest;
    public string KindLabel => Kind == StorySegmentKind.Transition ? "转场" : "剧情";

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(KindLabel))]
    private StorySegmentKind _kind;
    [ObservableProperty] private bool _isRefined;
    [ObservableProperty] private string _title;
    [ObservableProperty] private string _narrative;
    [ObservableProperty] private string _imagePrompt;
    [ObservableProperty] private string _videoPrompt;
    [ObservableProperty] private int _seconds;
    [ObservableProperty] private string _resourceIdsText;
    [ObservableProperty] private string _continuityIn;
    [ObservableProperty] private string _continuityOut;
    [ObservableProperty] private string _shotPlan;
    private string? _firstFrameAsset;
    private string? _lastFrameAsset;
    private string? _videoAsset;
    private string? _pendingVideoJobId;
    private string? _pendingVideoJobStatus;
    private string? _pendingVideoRequestDigest;

    public StorySegmentDocument ToDocument() => new(
        Id,
        Title.Trim(),
        Narrative.Trim(),
        ImagePrompt.Trim(),
        VideoPrompt.Trim(),
        Seconds,
        _firstFrameAsset,
        _lastFrameAsset,
        _videoAsset)
    {
        Kind = this.Kind,
        IsRefined = IsRefined,
        PendingVideoJobId = _pendingVideoJobId,
        PendingVideoJobStatus = _pendingVideoJobStatus,
        PendingVideoRequestDigest = _pendingVideoRequestDigest,
        ResourceIds = ParseResourceIds(ResourceIdsText),
        ContinuityIn = ContinuityIn.Trim(),
        ContinuityOut = ContinuityOut.Trim(),
        ShotPlan = ShotPlan.Trim(),
    };

    internal void RemoveResource(string resourceId)
    {
        ResourceIdsText = string.Join(", ", ParseResourceIds(ResourceIdsText)
            .Where(id => !string.Equals(id, resourceId, StringComparison.Ordinal)));
    }

    private static IReadOnlyList<string> ParseResourceIds(string value) => value
        .Split([',', '，', ';', '；'], StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries)
        .Distinct(StringComparer.Ordinal)
        .ToArray();

    public void SetFrame(bool lastFrame, string relativePath, string fullPath)
    {
        if (lastFrame)
        {
            _lastFrameAsset = relativePath;
            LastFramePath = fullPath;
            OnPropertyChanged(nameof(LastFramePath));
        }
        else
        {
            _firstFrameAsset = relativePath;
            FirstFramePath = fullPath;
            OnPropertyChanged(nameof(FirstFramePath));
        }
        OnPropertyChanged(nameof(FrameStatus));
    }

    public void SetVideo(string relativePath, string fullPath)
    {
        _videoAsset = relativePath;
        VideoPath = fullPath;
        ClearPendingVideoJob();
        OnPropertyChanged(nameof(VideoPath));
        OnPropertyChanged(nameof(VideoStatus));
    }

    public bool SetPendingVideoJob(string jobId, string status, string digest)
    {
        if (_pendingVideoJobId == jobId && _pendingVideoJobStatus == status &&
            _pendingVideoRequestDigest == digest) return false;
        _pendingVideoJobId = jobId;
        _pendingVideoJobStatus = status;
        _pendingVideoRequestDigest = digest;
        OnPropertyChanged(nameof(PendingVideoJobId));
        OnPropertyChanged(nameof(PendingVideoJobStatus));
        OnPropertyChanged(nameof(PendingVideoRequestDigest));
        OnPropertyChanged(nameof(HasPendingVideoJob));
        OnPropertyChanged(nameof(VideoStatus));
        return true;
    }

    public void ClearPendingVideoJob()
    {
        if (!HasPendingVideoJob && _pendingVideoJobStatus is null && _pendingVideoRequestDigest is null) return;
        _pendingVideoJobId = null;
        _pendingVideoJobStatus = null;
        _pendingVideoRequestDigest = null;
        OnPropertyChanged(nameof(PendingVideoJobId));
        OnPropertyChanged(nameof(PendingVideoJobStatus));
        OnPropertyChanged(nameof(PendingVideoRequestDigest));
        OnPropertyChanged(nameof(HasPendingVideoJob));
        OnPropertyChanged(nameof(VideoStatus));
    }

    internal void SetNumberLabel(string value)
    {
        NumberLabel = value;
        OnPropertyChanged(nameof(NumberLabel));
    }

    private void MarkRefinementStale()
    {
        if (IsRefined) IsRefined = false;
    }

    partial void OnKindChanged(StorySegmentKind value) => MarkRefinementStale();
    partial void OnTitleChanged(string value) => MarkRefinementStale();
    partial void OnNarrativeChanged(string value) => MarkRefinementStale();
    partial void OnImagePromptChanged(string value) => MarkRefinementStale();
    partial void OnVideoPromptChanged(string value) => MarkRefinementStale();
    partial void OnSecondsChanged(int value) => MarkRefinementStale();
    partial void OnResourceIdsTextChanged(string value) => MarkRefinementStale();
    partial void OnContinuityInChanged(string value) => MarkRefinementStale();
    partial void OnContinuityOutChanged(string value) => MarkRefinementStale();
    partial void OnShotPlanChanged(string value) => MarkRefinementStale();
}
