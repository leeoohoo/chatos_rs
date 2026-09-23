using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.QuickSearch;

public sealed partial class QuickSearchViewModel : ObservableObject, IDisposable
{
    private readonly IQuickSearchProvider[] _providers;
    private readonly IQuickSearchUsageStore _usageStore;
    private CancellationTokenSource? _searchCancellation;
    private long _searchGeneration;

    public QuickSearchViewModel(
        IEnumerable<IQuickSearchProvider> providers,
        IQuickSearchUsageStore usageStore)
    {
        _providers = providers.ToArray();
        _usageStore = usageStore;
    }

    public ObservableCollection<QuickSearchResult> Results { get; } = [];

    [ObservableProperty]
    private string _query = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(SelectedResult))]
    private int _selectedIndex = -1;

    [ObservableProperty]
    private bool _isSearching;

    [ObservableProperty]
    private string? _errorMessage;

    public QuickSearchResult? SelectedResult =>
        SelectedIndex >= 0 && SelectedIndex < Results.Count ? Results[SelectedIndex] : null;

    partial void OnQueryChanged(string value) => _ = SearchAsync(value);

    public async Task SearchAsync(string query, CancellationToken cancellationToken = default)
    {
        _searchCancellation?.Cancel();
        _searchCancellation?.Dispose();
        _searchCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var cancellation = _searchCancellation;
        var token = cancellation.Token;
        var generation = Interlocked.Increment(ref _searchGeneration);
        try
        {
            IsSearching = true;
            ErrorMessage = null;
            var (scope, normalizedQuery) = ParseScope(query);
            var providers = _providers.Where(provider => scope is null || provider.Kind == scope).ToArray();
            var searches = providers.Select(provider => provider.SearchAsync(normalizedQuery, token)).ToArray();
            var batches = await Task.WhenAll(searches);
            var usage = await _usageStore.LoadAsync(token);
            token.ThrowIfCancellationRequested();
            if (generation != Interlocked.Read(ref _searchGeneration)) return;

            var now = DateTimeOffset.UtcNow;
            var ranked = QuickSearchRanking.Sort(batches.SelectMany(static batch => batch).Select(result =>
            {
                if (!usage.TryGetValue(result.Id, out var item)) return result;
                var age = Math.Max(0, (now - item.LastUsedAt).TotalDays);
                var recency = Math.Max(0, 80 - age * 8);
                var frequency = Math.Min(45, Math.Sqrt(item.UseCount) * 10);
                return result with { Score = result.Score + recency + frequency };
            })).Take(80).ToArray();

            Results.Clear();
            foreach (var result in ranked) Results.Add(result);
            SelectedIndex = Results.Count == 0 ? -1 : 0;
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            // A newer query superseded this one.
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            if (generation == Interlocked.Read(ref _searchGeneration)) ErrorMessage = exception.Message;
        }
        finally
        {
            if (generation == Interlocked.Read(ref _searchGeneration)) IsSearching = false;
        }
    }

    public void MoveSelection(int delta)
    {
        if (Results.Count == 0) return;
        SelectedIndex = Math.Clamp(SelectedIndex + delta, 0, Results.Count - 1);
    }

    public Task RecordUseAsync(QuickSearchResult result, CancellationToken cancellationToken = default) =>
        _usageStore.RecordAsync(result.Id, cancellationToken);

    public void Reset()
    {
        Query = string.Empty;
        ErrorMessage = null;
        SelectedIndex = Results.Count == 0 ? -1 : 0;
    }

    internal static (QuickSearchResultKind? Scope, string Query) ParseScope(string value)
    {
        var trimmed = value.TrimStart();
        if (trimmed.Length == 0) return (null, string.Empty);
        return trimmed[0] switch
        {
            '>' => (QuickSearchResultKind.Action, trimmed[1..].TrimStart()),
            '@' => (QuickSearchResultKind.ChatOS, trimmed[1..].TrimStart()),
            '/' => (QuickSearchResultKind.File, trimmed[1..].TrimStart()),
            _ => (null, trimmed),
        };
    }

    public void Dispose()
    {
        _searchCancellation?.Cancel();
        _searchCancellation?.Dispose();
    }
}
