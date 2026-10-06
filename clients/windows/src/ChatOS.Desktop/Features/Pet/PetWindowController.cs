using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Presentation.Threading;

namespace ChatOS.Desktop.Features.Pet;

public sealed class PetWindowController : IDisposable
{
    private readonly PetWindow _window;
    private readonly AppPreferencesManager _preferences;
    private readonly IUiDispatcher _dispatcher;
    private readonly SemaphoreSlim _visibilityGate = new(1, 1);
    private bool _authenticated;

    public PetWindowController(
        PetWindow window,
        AppPreferencesManager preferences,
        IUiDispatcher dispatcher)
    {
        _window = window;
        _preferences = preferences;
        _dispatcher = dispatcher;
        _preferences.Changed += OnPreferencesChanged;
    }

    public async Task SetAuthenticatedAsync(
        bool authenticated,
        CancellationToken cancellationToken = default)
    {
        _authenticated = authenticated;
        await ApplyVisibilitySerializedAsync(_preferences.Current, cancellationToken);
    }

    public void Dispose()
    {
        _preferences.Changed -= OnPreferencesChanged;
        _authenticated = false;
        ObserveVisibilityChange(_preferences.Current, "pet window disposal");
    }

    private void OnPreferencesChanged(object? sender, AppPreferences preferences) =>
        ObserveVisibilityChange(preferences, "pet preference change");

    private void ObserveVisibilityChange(AppPreferences preferences, string phase) =>
        _ = ApplyObservedVisibilityAsync(preferences, phase);

    private async Task ApplyObservedVisibilityAsync(AppPreferences preferences, string phase)
    {
        try
        {
            await ApplyVisibilitySerializedAsync(preferences, CancellationToken.None)
                .ConfigureAwait(false);
        }
        catch (Exception exception)
        {
            StartupDiagnostics.RecordUnhandled(phase, exception);
        }
    }

    private async Task ApplyVisibilitySerializedAsync(
        AppPreferences preferences,
        CancellationToken cancellationToken)
    {
        await _visibilityGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            Task visibilityTask = Task.CompletedTask;
            await _dispatcher.InvokeAsync(
                () => visibilityTask = ApplyVisibilityAsync(preferences, cancellationToken),
                cancellationToken).ConfigureAwait(false);
            await visibilityTask.ConfigureAwait(false);
        }
        finally
        {
            _visibilityGate.Release();
        }
    }

    private async Task ApplyVisibilityAsync(
        AppPreferences preferences,
        CancellationToken cancellationToken)
    {
        if (_authenticated && preferences.PetEnabled)
        {
            await _window.ShowAsync(cancellationToken);
        }
        else
        {
            await _window.HidePetAsync();
        }
    }
}
