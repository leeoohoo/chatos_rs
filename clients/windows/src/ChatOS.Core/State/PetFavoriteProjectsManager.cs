using ChatOS.Core.Abstractions;

namespace ChatOS.Core.State;

public sealed class PetFavoriteProjectsManager
{
    private readonly IPetFavoriteProjectsStore _store;
    private readonly SemaphoreSlim _gate = new(1, 1);
    private HashSet<string> _projectIds = new(StringComparer.Ordinal);
    private bool _initialized;

    public PetFavoriteProjectsManager(IPetFavoriteProjectsStore store)
    {
        _store = store;
    }

    public event EventHandler? Changed;

    public IReadOnlySet<string> ProjectIds => _projectIds;

    public bool IsFavorite(string? projectId) =>
        !string.IsNullOrWhiteSpace(projectId) && _projectIds.Contains(projectId.Trim());

    public async Task InitializeAsync(CancellationToken cancellationToken = default)
    {
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_initialized) return;
            _projectIds = (await _store.LoadAsync(cancellationToken).ConfigureAwait(false))
                .Select(static value => value.Trim())
                .Where(static value => value.Length > 0)
                .ToHashSet(StringComparer.Ordinal);
            _initialized = true;
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task SetFavoriteAsync(
        string projectId,
        bool favorite,
        CancellationToken cancellationToken = default)
    {
        var normalized = projectId.Trim();
        if (normalized.Length == 0) throw new ArgumentException("Project id is required.", nameof(projectId));
        var changed = false;
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (!_initialized)
            {
                _projectIds = (await _store.LoadAsync(cancellationToken).ConfigureAwait(false))
                    .Select(static value => value.Trim())
                    .Where(static value => value.Length > 0)
                    .ToHashSet(StringComparer.Ordinal);
                _initialized = true;
            }

            var next = new HashSet<string>(_projectIds, StringComparer.Ordinal);
            changed = favorite ? next.Add(normalized) : next.Remove(normalized);
            if (!changed) return;
            await _store.SaveAsync(next.Order(StringComparer.Ordinal).ToArray(), cancellationToken)
                .ConfigureAwait(false);
            _projectIds = next;
        }
        finally
        {
            _gate.Release();
        }

        if (changed) Changed?.Invoke(this, EventArgs.Empty);
    }
}
