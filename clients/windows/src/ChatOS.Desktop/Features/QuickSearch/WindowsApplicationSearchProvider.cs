using ChatOS.Core.Domain;

namespace ChatOS.Desktop.Features.QuickSearch;

public sealed class WindowsApplicationSearchProvider : IQuickSearchProvider
{
    private readonly SemaphoreSlim _indexGate = new(1, 1);
    private ApplicationEntry[]? _index;

    public QuickSearchResultKind Kind => QuickSearchResultKind.Application;

    public async Task<IReadOnlyList<QuickSearchResult>> SearchAsync(
        string query,
        CancellationToken cancellationToken = default)
    {
        var index = await LoadIndexAsync(cancellationToken).ConfigureAwait(false);
        return index.Select(item =>
        {
            var score = QuickSearchRanking.Score(query, item.Name, item.Path, providerWeight: 25);
            return score is null ? null : new QuickSearchResult(
                $"application:{item.Path}", Kind, item.Name, item.Path, "\uE7B8", score.Value,
                new(QuickSearchActionKind.OpenApplication, item.Path));
        }).OfType<QuickSearchResult>().OrderByDescending(item => item.Score).Take(40).ToArray();
    }

    private async Task<ApplicationEntry[]> LoadIndexAsync(CancellationToken cancellationToken)
    {
        if (_index is not null) return _index;
        await _indexGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_index is not null) return _index;
            _index = await Task.Run(() => BuildIndex(cancellationToken), cancellationToken).ConfigureAwait(false);
            return _index;
        }
        finally
        {
            _indexGate.Release();
        }
    }

    private static ApplicationEntry[] BuildIndex(CancellationToken cancellationToken)
    {
        var roots = new[]
        {
            Environment.GetFolderPath(Environment.SpecialFolder.StartMenu),
            Environment.GetFolderPath(Environment.SpecialFolder.CommonStartMenu),
        };
        var entries = new Dictionary<string, ApplicationEntry>(StringComparer.OrdinalIgnoreCase);
        foreach (var root in roots.Where(Directory.Exists))
        {
            IEnumerable<string> paths;
            try
            {
                paths = Directory.EnumerateFiles(root, "*", SearchOption.AllDirectories)
                    .Where(path => Path.GetExtension(path) is ".lnk" or ".url" or ".exe");
            }
            catch (UnauthorizedAccessException)
            {
                continue;
            }
            foreach (var path in paths)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var name = Path.GetFileNameWithoutExtension(path);
                entries.TryAdd(name, new(name, path));
            }
        }
        return entries.Values.OrderBy(item => item.Name, StringComparer.CurrentCultureIgnoreCase).ToArray();
    }

    private sealed record ApplicationEntry(string Name, string Path);
}
