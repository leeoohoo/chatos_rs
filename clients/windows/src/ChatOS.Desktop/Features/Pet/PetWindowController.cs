using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Presentation.Threading;

namespace ChatOS.Desktop.Features.Pet;

public sealed class PetWindowController : IDisposable
{
    private readonly PetWindow _window;
    private readonly AppPreferencesManager _preferences;
    private readonly IUiDispatcher _dispatcher;
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
        await ApplyVisibilityAsync(_preferences.Current, cancellationToken);
    }

    public void Dispose()
    {
        _preferences.Changed -= OnPreferencesChanged;
        _ = _window.HidePetAsync();
    }

    private void OnPreferencesChanged(object? sender, AppPreferences preferences) =>
        _ = _dispatcher.InvokeAsync(() =>
        {
            _ = ApplyVisibilityAsync(preferences, CancellationToken.None);
        });

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
