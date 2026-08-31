using ChatOS.Core.Domain;

namespace ChatOS.Core.State;

public sealed class PetStateReducer
{
    private readonly Dictionary<string, PetActivity> _activities = new(StringComparer.Ordinal);
    private readonly HashSet<string> _seenEventIds = new(StringComparer.Ordinal);

    public bool Apply(PetActivityEvent activityEvent, DateTimeOffset? now = null)
    {
        var changed = activityEvent switch
        {
            PetActivityEvent.Upsert upsert => Upsert(upsert.Activity),
            PetActivityEvent.Remove remove => _activities.Remove(remove.Id),
            PetActivityEvent.RemoveSource removeSource => RemoveSource(removeSource.Source),
            PetActivityEvent.Reconcile => false,
            _ => false,
        };

        return RemoveExpired(now ?? DateTimeOffset.UtcNow) || changed;
    }

    public bool Replace(PetActivitySource source, IEnumerable<PetActivity> activities)
    {
        var next = activities
            .Where(activity => activity.Source == source)
            .ToDictionary(activity => activity.Id, StringComparer.Ordinal);

        var existingIds = _activities.Values
            .Where(activity => activity.Source == source)
            .Select(activity => activity.Id)
            .ToArray();

        var changed = existingIds.Length != next.Count || existingIds.Any(id => !next.ContainsKey(id));
        foreach (var id in existingIds)
        {
            _activities.Remove(id);
        }

        foreach (var activity in next.Values)
        {
            changed |= Upsert(activity);
        }

        return changed;
    }

    public bool RemoveExpired(DateTimeOffset? now = null)
    {
        var cutoff = now ?? DateTimeOffset.UtcNow;
        var expiredIds = _activities.Values
            .Where(activity => activity.ExpiresAt is { } expiration && expiration <= cutoff)
            .Select(activity => activity.Id)
            .ToArray();

        foreach (var id in expiredIds)
        {
            _activities.Remove(id);
        }

        return expiredIds.Length > 0;
    }

    public IReadOnlyList<PetActivity> VisibleActivities(DateTimeOffset? now = null)
    {
        RemoveExpired(now);
        return _activities.Values
            .OrderByDescending(activity => activity.PresentationPriority)
            .ThenByDescending(activity => activity.UpdatedAt)
            .ThenBy(activity => activity.Id, StringComparer.Ordinal)
            .ToArray();
    }

    public PetPresentation Presentation(DateTimeOffset? now = null)
    {
        var visible = VisibleActivities(now);
        var primary = visible.FirstOrDefault();
        return primary is null
            ? PetPresentation.Idle
            : new PetPresentation(
                primary.AnimationState,
                primary,
                visible.Count(activity => activity.Kind is PetActivityKind.Working or PetActivityKind.Reviewing),
                visible.Count(activity => activity.RequiresAttention));
    }

    public PetActivitySource? SourceForActivity(string id) =>
        _activities.TryGetValue(id, out var activity) ? activity.Source : null;

    public void Clear()
    {
        _activities.Clear();
        _seenEventIds.Clear();
    }

    private bool Upsert(PetActivity activity)
    {
        if (!string.IsNullOrWhiteSpace(activity.EventId) && !_seenEventIds.Add(activity.EventId))
        {
            return false;
        }

        if (_activities.TryGetValue(activity.Id, out var existing))
        {
            if (existing.EventSequence is { } existingSequence &&
                activity.EventSequence is { } incomingSequence &&
                incomingSequence < existingSequence)
            {
                return false;
            }

            if (existing == activity)
            {
                return false;
            }
        }

        _activities[activity.Id] = activity;
        return true;
    }

    private bool RemoveSource(PetActivitySource source)
    {
        var ids = _activities.Values
            .Where(activity => activity.Source == source)
            .Select(activity => activity.Id)
            .ToArray();

        foreach (var id in ids)
        {
            _activities.Remove(id);
        }

        return ids.Length > 0;
    }
}
