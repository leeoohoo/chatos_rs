using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.MediaStudio;

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
    public ObservableCollection<StoryProjectVideoItem> ProjectVideos { get; } = [];

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(PlaylistStatusLabel))]
    private bool _isPlaylistActive;

    public bool CanPlayStoryPlaylist => !IsBusy && ProjectVideos.Count > 0;
    public string ProjectVideoSummary => ProjectVideos.Count == 0
        ? "项目成片 · 暂无已完成视频"
        : $"项目成片 · {ProjectVideos.Count}/{Segments.Count} 段可播放";
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

    public void SelectPlaylistIndex(int index)
    {
        if (IsPlaylistActive && index >= 0 && index < ProjectVideos.Count)
            SelectedSegment = ProjectVideos[index].Segment;
    }

    private void RefreshProjectMedia()
    {
        ProjectVideos.Clear();
        for (var index = 0; index < Segments.Count; index++)
        {
            var segment = Segments[index];
            if (segment.VideoPath is { Length: > 0 } path && File.Exists(path))
                ProjectVideos.Add(new StoryProjectVideoItem(segment, index + 1));
        }
        if (ProjectVideos.Count == 0) IsPlaylistActive = false;
        OnPropertyChanged(nameof(CanPlayStoryPlaylist));
        OnPropertyChanged(nameof(ProjectVideoSummary));
        OnPropertyChanged(nameof(PlaylistStatusLabel));
    }
}
