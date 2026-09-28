using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed class StoryProjectImageItem
{
    public StoryProjectImageItem(
        string title,
        string kindLabel,
        string filePath,
        StoryResourceEditor? resource,
        StorySegmentEditor? segment)
    {
        Title = title;
        KindLabel = kindLabel;
        FilePath = filePath;
        Resource = resource;
        Segment = segment;
    }

    public string Title { get; }
    public string KindLabel { get; }
    public string FilePath { get; }
    public StoryResourceEditor? Resource { get; }
    public StorySegmentEditor? Segment { get; }
}

public sealed class StoryProjectVideoItem
{
    public StoryProjectVideoItem(StorySegmentEditor segment, int sequence)
    {
        Segment = segment;
        Sequence = sequence;
    }

    public StorySegmentEditor Segment { get; }
    public int Sequence { get; }
    public string Title => Segment.Title;
    public string FilePath => Segment.VideoPath!;
    public string SequenceLabel => $"第 {Sequence} 段 · {Segment.Seconds} 秒";
}

public sealed partial class StoryStudioViewModel
{
    public ObservableCollection<StoryProjectImageItem> ProjectImages { get; } = [];
    public ObservableCollection<StoryProjectVideoItem> ProjectVideos { get; } = [];

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(PlaylistStatusLabel))]
    private bool _isPlaylistActive;

    public bool CanPlayStoryPlaylist => !IsBusy && ProjectVideos.Count > 0;
    public string ProjectVideoSummary =>
        $"项目作品 · {ProjectImages.Count} 张图片 · {ProjectVideos.Count}/{Segments.Count} 段视频";
    public string PlaylistStatusLabel => IsPlaylistActive
        ? $"正在按剧情顺序连续播放 {ProjectVideos.Count} 个已完成分段"
        : "只播放当前项目已有成片，缺失分段会自动跳过。";

    public IReadOnlyList<StoryProjectVideoItem> StartStoryPlaylist()
    {
        if (!CanPlayStoryPlaylist) return [];
        IsPlaylistActive = true;
        SelectedSegment = ProjectVideos[0].Segment;
        return ProjectVideos.ToArray();
    }

    public void StopStoryPlaylist() => IsPlaylistActive = false;

    public void SelectProjectVideo(StoryProjectVideoItem item)
    {
        IsPlaylistActive = false;
        if (Segments.Contains(item.Segment)) SelectedSegment = item.Segment;
    }

    public void SelectProjectImage(StoryProjectImageItem item)
    {
        if (IsBusy) return;
        if (item.Resource is not null && Resources.Contains(item.Resource)) SelectedResource = item.Resource;
        if (item.Segment is not null && Segments.Contains(item.Segment)) SelectedSegment = item.Segment;
    }

    public void SelectPlaylistIndex(int index)
    {
        if (IsPlaylistActive && index >= 0 && index < ProjectVideos.Count)
            SelectedSegment = ProjectVideos[index].Segment;
    }

    private void RefreshProjectMedia()
    {
        ProjectImages.Clear();
        ProjectVideos.Clear();
        foreach (var resource in Resources)
        {
            if (resource.ImagePath is { Length: > 0 } path && File.Exists(path))
                ProjectImages.Add(new StoryProjectImageItem(
                    resource.Name, $"{resource.KindLabel}参考图", path, resource, null));
        }
        for (var index = 0; index < Segments.Count; index++)
        {
            var segment = Segments[index];
            if (segment.FirstFramePath is { Length: > 0 } first && File.Exists(first))
                ProjectImages.Add(new StoryProjectImageItem(
                    segment.Title, $"第 {index + 1} 段首帧", first, null, segment));
            if (segment.LastFramePath is { Length: > 0 } last && File.Exists(last))
                ProjectImages.Add(new StoryProjectImageItem(
                    segment.Title, $"第 {index + 1} 段尾帧", last, null, segment));
            if (segment.ActualVideoLastFramePath is { Length: > 0 } actualLast && File.Exists(actualLast))
                ProjectImages.Add(new StoryProjectImageItem(
                    segment.Title, $"第 {index + 1} 段成片末帧", actualLast, null, segment));
            if (segment.VideoPath is { Length: > 0 } path && File.Exists(path))
                ProjectVideos.Add(new StoryProjectVideoItem(segment, index + 1));
        }
        if (ProjectVideos.Count == 0) IsPlaylistActive = false;
        OnPropertyChanged(nameof(CanPlayStoryPlaylist));
        OnPropertyChanged(nameof(ProjectVideoSummary));
        OnPropertyChanged(nameof(PlaylistStatusLabel));
    }
}
