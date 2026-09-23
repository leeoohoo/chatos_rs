namespace ChatOS.Core.Abstractions;

public sealed record QuickSearchUsage(int UseCount, DateTimeOffset LastUsedAt);

public interface IQuickSearchUsageStore
{
    Task<IReadOnlyDictionary<string, QuickSearchUsage>> LoadAsync(
        CancellationToken cancellationToken = default);

    Task RecordAsync(string resultId, CancellationToken cancellationToken = default);
}
