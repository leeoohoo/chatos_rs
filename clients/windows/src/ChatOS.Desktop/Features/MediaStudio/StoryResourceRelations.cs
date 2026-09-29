using System.Collections.ObjectModel;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed class StorySegmentRelationLink
{
    public StorySegmentRelationLink(StorySegmentEditor segment)
    {
        Segment = segment;
        Label = $"{segment.NumberLabel} · {segment.Title}";
    }

    public StorySegmentEditor Segment { get; }
    public string Label { get; }
}

public sealed class StoryResourceRelationGroup
{
    public StoryResourceRelationGroup(
        string resourceId,
        string header,
        string kindLabel,
        StoryResourceEditor? resource,
        IEnumerable<StorySegmentEditor> segments)
    {
        ResourceId = resourceId;
        Header = header;
        KindLabel = kindLabel;
        Resource = resource;
        Links = new ObservableCollection<StorySegmentRelationLink>(
            segments.Select(segment => new StorySegmentRelationLink(segment)));
    }

    public string ResourceId { get; }
    public string Header { get; }
    public string KindLabel { get; }
    public StoryResourceEditor? Resource { get; }
    public bool CanSelectResource => Resource is not null;
    public ObservableCollection<StorySegmentRelationLink> Links { get; }
    public string UsageLabel => Links.Count == 0 ? "没有分段使用" : $"{Links.Count} 个分段使用";
}

public sealed partial class StoryStudioViewModel
{
    public ObservableCollection<StoryResourceRelationGroup> ResourceRelations { get; } = [];

    public string RelationSummary
    {
        get
        {
            var linked = ResourceRelations
                .Where(group => group.Resource is not null)
                .Sum(group => group.Links.Count);
            var unlinked = ResourceRelations
                .FirstOrDefault(group => group.ResourceId == UnlinkedGroupId)?.Links.Count ?? 0;
            return $"素材关系 · {linked} 条关联 · {unlinked} 个未关联分段";
        }
    }

    public void SelectRelationResource(StoryResourceRelationGroup group)
    {
        if (!IsBusy && group.Resource is not null) SelectedResource = group.Resource;
    }

    public void SelectRelationSegment(StorySegmentRelationLink link)
    {
        if (!IsBusy && Segments.Contains(link.Segment)) SelectedSegment = link.Segment;
    }

    private void RefreshStoryRelations()
    {
        ResourceRelations.Clear();
        if (_current is null)
        {
            OnPropertyChanged(nameof(RelationSummary));
            return;
        }

        var resourceById = Resources.ToDictionary(resource => resource.Id, StringComparer.Ordinal);
        var segmentIds = Segments.ToDictionary(
            segment => segment,
            segment => segment.ToDocument().ResourceIds.ToHashSet(StringComparer.Ordinal));
        foreach (var resource in Resources)
        {
            ResourceRelations.Add(new StoryResourceRelationGroup(
                resource.Id,
                resource.Name,
                resource.KindLabel,
                resource,
                Segments.Where(segment => segmentIds[segment].Contains(resource.Id))));
        }

        var unknownIds = segmentIds.Values
            .SelectMany(ids => ids)
            .Where(id => !resourceById.ContainsKey(id))
            .Distinct(StringComparer.Ordinal)
            .OrderBy(id => id, StringComparer.Ordinal);
        foreach (var unknownId in unknownIds)
        {
            ResourceRelations.Add(new StoryResourceRelationGroup(
                unknownId,
                unknownId,
                "素材 ID 不存在",
                null,
                Segments.Where(segment => segmentIds[segment].Contains(unknownId))));
        }

        var unlinked = Segments.Where(segment => segmentIds[segment].Count == 0).ToArray();
        if (unlinked.Length > 0)
        {
            ResourceRelations.Add(new StoryResourceRelationGroup(
                UnlinkedGroupId,
                "未关联素材",
                "需要补充素材 ID",
                null,
                unlinked));
        }
        OnPropertyChanged(nameof(RelationSummary));
    }

    private const string UnlinkedGroupId = "__unlinked__";
}
