using ChatOS.Core.Domain;
using Windows.Storage;
using Windows.Storage.Search;

namespace ChatOS.Desktop.Features.QuickSearch;

public sealed class WindowsFileSearchProvider : IQuickSearchProvider
{
    public QuickSearchResultKind Kind => QuickSearchResultKind.File;

    public async Task<IReadOnlyList<QuickSearchResult>> SearchAsync(
        string query,
        CancellationToken cancellationToken = default)
    {
        if (query.Trim().Length < 2) return [];
        var results = new Dictionary<string, QuickSearchResult>(StringComparer.OrdinalIgnoreCase);
        foreach (var folderFactory in GetKnownFolderFactories())
        {
            cancellationToken.ThrowIfCancellationRequested();
            try
            {
                var folder = folderFactory();
                var options = new QueryOptions(CommonFileQuery.OrderByName, ["*"])
                {
                    FolderDepth = FolderDepth.Deep,
                    IndexerOption = IndexerOption.UseIndexerWhenAvailable,
                    UserSearchFilter = query.Trim(),
                };
                var files = await folder.CreateFileQueryWithOptions(options).GetFilesAsync(0, 40);
                cancellationToken.ThrowIfCancellationRequested();
                foreach (var file in files)
                {
                    var score = QuickSearchRanking.Score(query, file.DisplayName, file.Path, providerWeight: 10);
                    if (score is null) continue;
                    results.TryAdd(file.Path, new(
                        $"file:{file.Path}", Kind, file.Name, file.Path, "\uE8A5", score.Value,
                        new(QuickSearchActionKind.OpenFile, file.Path)));
                }
            }
            catch (Exception exception) when (exception is not OperationCanceledException)
            {
                // A library can be unavailable or redirected on a given Windows installation.
            }
        }
        return results.Values.OrderByDescending(item => item.Score).Take(50).ToArray();
    }

    private static IEnumerable<Func<StorageFolder>> GetKnownFolderFactories() =>
    [
        static () => KnownFolders.DocumentsLibrary,
        static () => KnownFolders.PicturesLibrary,
        static () => KnownFolders.VideosLibrary,
        static () => KnownFolders.MusicLibrary,
    ];
}
