using ChatOS.Core.Domain;
using ChatOS.Core.State;
using CommunityToolkit.Mvvm.ComponentModel;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Settings;

public sealed partial class AppSettingsViewModel : ObservableObject
{
    private readonly AppPreferencesManager _preferences;
    private readonly IUiDispatcher _dispatcher;

    public AppSettingsViewModel(AppPreferencesManager preferences, IUiDispatcher dispatcher)
    {
        _preferences = preferences;
        _dispatcher = dispatcher;
        Apply(_preferences.Current);
        _preferences.Changed += OnPreferencesChanged;
    }

    [ObservableProperty]
    private InterfaceLanguage _language;

    [ObservableProperty]
    private InterfaceTheme _theme;

    [ObservableProperty]
    private double _fontScale = 1.0;

    [ObservableProperty]
    private bool _petEnabled = true;

    public Task SetLanguageAsync(InterfaceLanguage language, CancellationToken cancellationToken = default) =>
        _preferences.UpdateAsync(value => value with { Language = language }, cancellationToken);

    public Task SetThemeAsync(InterfaceTheme theme, CancellationToken cancellationToken = default) =>
        _preferences.UpdateAsync(value => value with { Theme = theme }, cancellationToken);

    public Task SetFontScaleAsync(double fontScale, CancellationToken cancellationToken = default) =>
        _preferences.UpdateAsync(value => value with { FontScale = fontScale }, cancellationToken);

    public Task SetPetEnabledAsync(bool enabled, CancellationToken cancellationToken = default) =>
        _preferences.UpdateAsync(value => value with { PetEnabled = enabled }, cancellationToken);

    private async void OnPreferencesChanged(object? sender, AppPreferences preferences) =>
        await _dispatcher.InvokeAsync(() => Apply(preferences));

    private void Apply(AppPreferences preferences)
    {
        Language = preferences.Language;
        Theme = preferences.Theme;
        FontScale = preferences.FontScale;
        PetEnabled = preferences.PetEnabled;
    }
}
