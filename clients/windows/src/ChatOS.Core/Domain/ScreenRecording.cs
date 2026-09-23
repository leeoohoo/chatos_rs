namespace ChatOS.Core.Domain;

public enum ScreenRecordingState
{
    Idle,
    SelectingTarget,
    RecordingOrAwaitingSave,
    Archiving,
    Completed,
    Failed,
}

public sealed record ScreenRecordingFileCandidate(
    string Path,
    long Length,
    DateTimeOffset LastWriteTime);

public sealed class ScreenRecordingArchiveTracker(DateTimeOffset startedAt)
{
    private readonly Dictionary<string, ScreenRecordingFileCandidate> _observed =
        new(StringComparer.OrdinalIgnoreCase);

    public ScreenRecordingFileCandidate? Observe(
        IEnumerable<ScreenRecordingFileCandidate> candidates,
        DateTimeOffset now)
    {
        foreach (var candidate in candidates
                     .Where(IsEligible)
                     .OrderByDescending(value => value.LastWriteTime))
        {
            if (_observed.TryGetValue(candidate.Path, out var previous)
                && previous.Length == candidate.Length
                && previous.LastWriteTime == candidate.LastWriteTime
                && now - candidate.LastWriteTime >= TimeSpan.FromSeconds(2))
            {
                return candidate;
            }

            _observed[candidate.Path] = candidate;
        }

        return null;
    }

    private bool IsEligible(ScreenRecordingFileCandidate candidate) =>
        candidate.Length > 0
        && candidate.LastWriteTime >= startedAt - TimeSpan.FromSeconds(2)
        && string.Equals(System.IO.Path.GetExtension(candidate.Path), ".mp4", StringComparison.OrdinalIgnoreCase);
}
