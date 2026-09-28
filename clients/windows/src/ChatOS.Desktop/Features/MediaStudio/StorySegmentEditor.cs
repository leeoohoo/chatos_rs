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
    public string VideoStatus => VideoPath is { Length: > 0 } ? "视频已完成" : "视频待生成";

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
        OnPropertyChanged(nameof(VideoPath));
        OnPropertyChanged(nameof(VideoStatus));
    }

    internal void SetNumberLabel(string value)
    {
        NumberLabel = value;
        OnPropertyChanged(nameof(NumberLabel));
    }
}
