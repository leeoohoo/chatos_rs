using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Core.State;

public sealed class AppPreferencesManager
{
    private readonly IAppPreferencesStore _store;
    private readonly SemaphoreSlim _gate = new(1, 1);
    private AppPreferences _current = AppPreferences.Default;
    private bool _initialized;

    public AppPreferencesManager(IAppPreferencesStore store)
    {
        _store = store;
    }

    public AppPreferences Current => _current;

    public event EventHandler<AppPreferences>? Changed;

    public async Task InitializeAsync(CancellationToken cancellationToken = default)
    {
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_initialized)
            {
                return;
            }

            _current = (await _store.LoadAsync(cancellationToken).ConfigureAwait(false)
                ?? AppPreferences.Default).Normalize();
            _initialized = true;
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task UpdateAsync(
        Func<AppPreferences, AppPreferences> update,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(update);
        AppPreferences updated;
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (!_initialized)
            {
                _current = (await _store.LoadAsync(cancellationToken).ConfigureAwait(false)
                    ?? AppPreferences.Default).Normalize();
                _initialized = true;
            }

            updated = update(_current).Normalize();
            if (updated == _current)
            {
                return;
            }

            await _store.SaveAsync(updated, cancellationToken).ConfigureAwait(false);
            _current = updated;
        }
        finally
        {
            _gate.Release();
        }

        Changed?.Invoke(this, updated);
    }
}
