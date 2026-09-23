using ChatOS.Core.Domain;
using ChatOS.Desktop.AppShell;

namespace ChatOS.Desktop.Features.QuickSearch;

public sealed class ChatOSQuickSearchProvider(MainWindowViewModel mainWindow) : IQuickSearchProvider
{
    public QuickSearchResultKind Kind => QuickSearchResultKind.ChatOS;

    public Task<IReadOnlyList<QuickSearchResult>> SearchAsync(
        string query,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var projects = mainWindow.Projects.Select(item => Create(item, query, QuickSearchActionKind.OpenProject, 55));
        var contacts = mainWindow.Contacts.Select(item => Create(item, query, QuickSearchActionKind.OpenContact, 45));
        return Task.FromResult<IReadOnlyList<QuickSearchResult>>(projects.Concat(contacts)
            .OfType<QuickSearchResult>().ToArray());
    }

    private static QuickSearchResult? Create(
        ShellResourceViewModel item,
        string query,
        QuickSearchActionKind action,
        double weight)
    {
        var score = QuickSearchRanking.Score(query, item.Title, item.Subtitle, providerWeight: weight);
        return score is null ? null : new(
            $"chatos:{item.Id}", QuickSearchResultKind.ChatOS, item.Title, item.Subtitle,
            item.Glyph, score.Value, new(action, item.Id));
    }
}
