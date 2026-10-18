using ChatOS.Core.Domain;
using ChatOS.Core.State;
using CommunityToolkit.Mvvm.ComponentModel;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Settings;

public sealed partial class AppSettingsViewModel : ObservableObject
{
    private readonly AppPreferencesManager _preferences;
    private readonly IUiDispatcher _dispatcher;
    private long _saveGeneration;
    private int _pendingSaves;

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

    [ObservableProperty]
    private double _petSize = AppPreferences.DefaultPetSize;

    [ObservableProperty]
    private bool _petShowProcess = true;

    [ObservableProperty]
    private bool _petShowCompletions = true;

    [ObservableProperty]
    private bool _globalUtilitiesEnabled = true;

    [ObservableProperty]
    private bool _globalScreenshotEnabled = true;

    [ObservableProperty]
    private bool _globalRecordingEnabled = true;

    [ObservableProperty]
    private bool _globalClipboardEnabled = true;

    [ObservableProperty]
    private bool _globalQuickSearchEnabled = true;

    [ObservableProperty]
    private bool _isSaving;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasError))]
    private string _errorMessage = string.Empty;

    public bool HasError => !string.IsNullOrEmpty(ErrorMessage);

    public Task SetLanguageAsync(InterfaceLanguage language, CancellationToken cancellationToken = default) =>
        UpdateAsync(value => value with { Language = language }, cancellationToken);

    public Task SetThemeAsync(InterfaceTheme theme, CancellationToken cancellationToken = default) =>
        UpdateAsync(value => value with { Theme = theme }, cancellationToken);

    public Task SetFontScaleAsync(double fontScale, CancellationToken cancellationToken = default) =>
        UpdateAsync(value => value with { FontScale = fontScale }, cancellationToken);

    public Task ResetFontScaleAsync(CancellationToken cancellationToken = default) =>
        SetFontScaleAsync(AppPreferences.Default.FontScale, cancellationToken);

    public Task SetPetEnabledAsync(bool enabled, CancellationToken cancellationToken = default) =>
        UpdateAsync(value => value with { PetEnabled = enabled }, cancellationToken);

    public Task SetPetSizeAsync(double size, CancellationToken cancellationToken = default) =>
        UpdateAsync(value => value with { PetSize = size }, cancellationToken);

    public Task SetPetShowProcessAsync(bool enabled, CancellationToken cancellationToken = default) =>
        UpdateAsync(value => value with { PetShowProcess = enabled }, cancellationToken);

    public Task SetPetShowCompletionsAsync(bool enabled, CancellationToken cancellationToken = default) =>
        UpdateAsync(value => value with { PetShowCompletions = enabled }, cancellationToken);

    public Task SetGlobalUtilitiesEnabledAsync(bool enabled, CancellationToken cancellationToken = default) =>
        UpdateAsync(value => value with { GlobalUtilitiesEnabled = enabled }, cancellationToken);

    public Task SetGlobalScreenshotEnabledAsync(bool enabled, CancellationToken cancellationToken = default) =>
        UpdateAsync(value => value with { GlobalScreenshotEnabled = enabled }, cancellationToken);

    public Task SetGlobalRecordingEnabledAsync(bool enabled, CancellationToken cancellationToken = default) =>
        UpdateAsync(value => value with { GlobalRecordingEnabled = enabled }, cancellationToken);

    public Task SetGlobalClipboardEnabledAsync(bool enabled, CancellationToken cancellationToken = default) =>
        UpdateAsync(value => value with { GlobalClipboardEnabled = enabled }, cancellationToken);

    public Task SetGlobalQuickSearchEnabledAsync(bool enabled, CancellationToken cancellationToken = default) =>
        UpdateAsync(value => value with { GlobalQuickSearchEnabled = enabled }, cancellationToken);

    public Task ResetGlobalUtilityDefaultsAsync(CancellationToken cancellationToken = default) =>
        UpdateAsync(value => value with
        {
            GlobalUtilitiesEnabled = AppPreferences.Default.GlobalUtilitiesEnabled,
            GlobalScreenshotEnabled = AppPreferences.Default.GlobalScreenshotEnabled,
            GlobalRecordingEnabled = AppPreferences.Default.GlobalRecordingEnabled,
            GlobalClipboardEnabled = AppPreferences.Default.GlobalClipboardEnabled,
            GlobalQuickSearchEnabled = AppPreferences.Default.GlobalQuickSearchEnabled,
        }, cancellationToken);

    private async Task UpdateAsync(
        Func<AppPreferences, AppPreferences> update,
        CancellationToken cancellationToken)
    {
        var generation = Interlocked.Increment(ref _saveGeneration);
        await _dispatcher.InvokeAsync(() =>
        {
            _pendingSaves++;
            IsSaving = true;
        });
        var error = string.Empty;
        try
        {
            await _preferences.UpdateAsync(update, cancellationToken);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            // A cancelled request is not a storage failure.
        }
        catch (Exception exception)
        {
            error = exception.Message;
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                // Use the committed state even if newer writes completed while queued.
                Apply(_preferences.Current);
                if (generation == Volatile.Read(ref _saveGeneration))
                    ErrorMessage = error;
                IsSaving = --_pendingSaves > 0;
                // The control may have an unsaved value even when no VM property changed.
                OnPropertyChanged(string.Empty);
            });
        }
    }

    private async void OnPreferencesChanged(object? sender, AppPreferences preferences) =>
        await _dispatcher.InvokeAsync(() => Apply(_preferences.Current));

    private void Apply(AppPreferences preferences)
    {
        Language = preferences.Language;
        Theme = preferences.Theme;
        FontScale = preferences.FontScale;
        PetEnabled = preferences.PetEnabled;
        PetSize = preferences.PetSize;
        PetShowProcess = preferences.PetShowProcess;
        PetShowCompletions = preferences.PetShowCompletions;
        GlobalUtilitiesEnabled = preferences.GlobalUtilitiesEnabled;
        GlobalScreenshotEnabled = preferences.GlobalScreenshotEnabled;
        GlobalRecordingEnabled = preferences.GlobalRecordingEnabled;
        GlobalClipboardEnabled = preferences.GlobalClipboardEnabled;
        GlobalQuickSearchEnabled = preferences.GlobalQuickSearchEnabled;
    }
}
