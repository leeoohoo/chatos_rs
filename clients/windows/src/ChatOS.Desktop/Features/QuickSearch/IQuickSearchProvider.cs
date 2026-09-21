using ChatOS.Core.Domain;

namespace ChatOS.Desktop.Features.QuickSearch;

public interface IQuickSearchProvider
{
    QuickSearchResultKind Kind { get; }

    Task<IReadOnlyList<QuickSearchResult>> SearchAsync(
        string query,
        CancellationToken cancellationToken = default);
}
