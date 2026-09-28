namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class StoryStudioViewModel
{
    public bool CanMoveSegmentUp => CanReorderSegments && SelectedSegment is not null &&
        Segments.IndexOf(SelectedSegment) > 0;
    public bool CanMoveSegmentDown => CanReorderSegments && SelectedSegment is { } segment &&
        Segments.IndexOf(segment) is var index && index >= 0 && index + 1 < Segments.Count;

    private bool CanReorderSegments => !IsBusy &&
        Segments.All(segment => string.IsNullOrWhiteSpace(segment.VideoPath));

    public void MoveSelectedSegmentUp() => MoveSelectedSegment(-1);

    public void MoveSelectedSegmentDown() => MoveSelectedSegment(1);

    private void MoveSelectedSegment(int offset)
    {
        var segment = SelectedSegment;
        if (segment is null || !CanReorderSegments) return;
        var oldIndex = Segments.IndexOf(segment);
        var newIndex = oldIndex + offset;
        if (oldIndex < 0 || newIndex < 0 || newIndex >= Segments.Count) return;
        Segments.Move(oldIndex, newIndex);
        RepairContinuityRange(Math.Min(oldIndex, newIndex) - 1, Math.Max(oldIndex, newIndex) + 1);
        SelectedSegment = segment;
        StatusMessage = $"已将“{segment.Title}”移动到第 {newIndex + 1} 段，并重建相邻衔接";
        NotifySegmentOrderChanged();
    }

    private void RepairContinuityRange(int start, int end)
    {
        if (Segments.Count == 0) return;
        start = Math.Clamp(start, 0, Segments.Count - 1);
        end = Math.Clamp(end, start, Segments.Count - 1);
        for (var index = start; index <= end; index++)
        {
            Segments[index].ContinuityIn = string.Empty;
            Segments[index].ContinuityOut = string.Empty;
        }
        FillMissingContinuity();
        RefreshContinuityAudit();
        RefreshPromptAudit();
    }

    private void NotifySegmentOrderChanged()
    {
        OnPropertyChanged(nameof(CanMoveSegmentUp));
        OnPropertyChanged(nameof(CanMoveSegmentDown));
    }
}
