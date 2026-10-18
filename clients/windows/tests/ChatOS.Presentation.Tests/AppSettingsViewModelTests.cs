using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class AppSettingsViewModelTests
{
    [Theory]
    [InlineData("language")]
    [InlineData("theme")]
    [InlineData("font")]
    [InlineData("pet")]
    [InlineData("petSize")]
    [InlineData("process")]
    [InlineData("completion")]
    [InlineData("utilities")]
    [InlineData("screenshot")]
    [InlineData("recording")]
    [InlineData("clipboard")]
    [InlineData("search")]
    public async Task Failed_save_keeps_persisted_values_and_requests_control_resync(string setting)
    {
        var store = new PreferencesStore { Failure = new IOException("disk unavailable") };
        var manager = new AppPreferencesManager(store);
        await manager.InitializeAsync();
        var viewModel = new AppSettingsViewModel(manager, new ImmediateUiDispatcher());
        var resync = false;
        viewModel.PropertyChanged += (_, args) => resync |= string.IsNullOrEmpty(args.PropertyName);

        var error = await Record.ExceptionAsync(() => ChangeAsync(viewModel, setting));

        Assert.Null(error);
        Assert.Equal(AppPreferences.Default, manager.Current);
        Assert.Equal(AppPreferences.Default, store.Value);
        Assert.True(resync);
        Assert.True(viewModel.HasError);
        Assert.Equal("disk unavailable", viewModel.ErrorMessage);
        Assert.False(viewModel.IsSaving);
    }

    [Fact]
    public async Task Retry_after_storage_failure_persists_the_requested_value()
    {
        var store = new PreferencesStore { Failure = new IOException("disk unavailable") };
        var manager = new AppPreferencesManager(store);
        var viewModel = new AppSettingsViewModel(manager, new ImmediateUiDispatcher());
        Assert.Null(await Record.ExceptionAsync(() => viewModel.SetThemeAsync(InterfaceTheme.Dark)));
        Assert.True(viewModel.HasError);
        store.Failure = null;

        await viewModel.SetThemeAsync(InterfaceTheme.Dark);

        Assert.Equal(InterfaceTheme.Dark, store.Value.Theme);
        Assert.Equal(InterfaceTheme.Dark, viewModel.Theme);
        Assert.False(viewModel.HasError);
    }

    [Fact]
    public async Task Saving_indicator_tracks_all_pending_writes_and_publishes_only_saved_values()
    {
        var started = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var store = new PreferencesStore { BeforeSave = async () => { started.TrySetResult(); await release.Task; } };
        var viewModel = new AppSettingsViewModel(new AppPreferencesManager(store), new ImmediateUiDispatcher());

        var first = viewModel.SetThemeAsync(InterfaceTheme.Dark);
        await started.Task.WaitAsync(TimeSpan.FromSeconds(5));
        var second = viewModel.SetLanguageAsync(InterfaceLanguage.English);
        Assert.True(viewModel.IsSaving);
        Assert.Equal(InterfaceTheme.System, viewModel.Theme);
        Assert.Equal(AppPreferences.Default, store.Value);
        release.SetResult();
        await Task.WhenAll(first, second);

        Assert.False(viewModel.IsSaving);
        Assert.Equal(InterfaceTheme.Dark, viewModel.Theme);
        Assert.Equal(InterfaceLanguage.English, viewModel.Language);
        Assert.Equal(2, store.SaveCount);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task Cancellation_restores_controls_without_a_storage_error(bool inFlight)
    {
        using var cancellation = new CancellationTokenSource();
        var store = new PreferencesStore();
        if (inFlight) store.BeforeSave = () => { cancellation.Cancel(); return Task.CompletedTask; };
        else cancellation.Cancel();
        var viewModel = new AppSettingsViewModel(new AppPreferencesManager(store), new ImmediateUiDispatcher());

        await viewModel.SetThemeAsync(InterfaceTheme.Dark, cancellation.Token);

        Assert.Equal(AppPreferences.Default, store.Value);
        Assert.Equal(InterfaceTheme.System, viewModel.Theme);
        Assert.False(viewModel.IsSaving);
        Assert.False(viewModel.HasError);
    }

    [Theory]
    [InlineData(0.85)]
    [InlineData(1.30)]
    public async Task Font_reset_changes_only_font_scale_and_survives_reopening(double fontScale)
    {
        var initial = AppPreferences.Default with
        {
            FontScale = fontScale, Language = InterfaceLanguage.English,
            Theme = InterfaceTheme.Dark, GlobalClipboardEnabled = false, PetEnabled = false,
        };
        var store = new PreferencesStore { Value = initial };
        var manager = new AppPreferencesManager(store);
        await manager.InitializeAsync();
        var viewModel = new AppSettingsViewModel(manager, new ImmediateUiDispatcher());

        await viewModel.ResetFontScaleAsync();

        Assert.Equal(initial with { FontScale = AppPreferences.Default.FontScale }, store.Value);
        var reopened = new AppPreferencesManager(store);
        await reopened.InitializeAsync();
        Assert.Equal(store.Value, reopened.Current);
        Assert.Equal(1, store.SaveCount);
    }

    [Fact]
    public async Task Utilities_reset_uses_one_write_and_preserves_other_preferences()
    {
        var initial = AppPreferences.Default with
        {
            FontScale = 1.2, Theme = InterfaceTheme.Dark, PetEnabled = false,
            GlobalUtilitiesEnabled = false, GlobalScreenshotEnabled = false,
            GlobalRecordingEnabled = false, GlobalClipboardEnabled = false, GlobalQuickSearchEnabled = false,
        };
        var store = new PreferencesStore { Value = initial };
        var viewModel = new AppSettingsViewModel(new AppPreferencesManager(store), new ImmediateUiDispatcher());

        await viewModel.ResetGlobalUtilityDefaultsAsync();

        Assert.Equal(AppPreferences.Default with { FontScale = 1.2, Theme = InterfaceTheme.Dark, PetEnabled = false }, store.Value);
        Assert.Equal(1, store.SaveCount);
        Assert.True(viewModel.GlobalUtilitiesEnabled);
        Assert.True(viewModel.GlobalQuickSearchEnabled);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task Failed_reset_keeps_original_preferences_and_can_be_retried(bool utilities)
    {
        var initial = AppPreferences.Default with { FontScale = 1.3, GlobalScreenshotEnabled = false, GlobalClipboardEnabled = false };
        var store = new PreferencesStore { Value = initial, Failure = new IOException("disk unavailable") };
        var viewModel = new AppSettingsViewModel(new AppPreferencesManager(store), new ImmediateUiDispatcher());
        Task Reset() => utilities ? viewModel.ResetGlobalUtilityDefaultsAsync() : viewModel.ResetFontScaleAsync();

        await Reset();

        Assert.Equal(initial, store.Value);
        Assert.True(viewModel.HasError);
        store.Failure = null;
        await Reset();
        Assert.False(viewModel.HasError);
        Assert.Equal(utilities ? initial.FontScale : 1, store.Value.FontScale);
        Assert.Equal(utilities, store.Value.GlobalScreenshotEnabled);
    }

    [Fact]
    public async Task Delayed_preference_notification_does_not_restore_an_older_value()
    {
        var manager = new AppPreferencesManager(new PreferencesStore());
        var dispatcher = new DeferredUiDispatcher { DeferNext = true };
        var viewModel = new AppSettingsViewModel(manager, dispatcher);

        await manager.UpdateAsync(value => value with { Theme = InterfaceTheme.Dark });
        await manager.UpdateAsync(value => value with { Theme = InterfaceTheme.Light });
        Assert.Equal(InterfaceTheme.Light, viewModel.Theme);
        dispatcher.Release();

        Assert.Equal(InterfaceTheme.Light, viewModel.Theme);
    }

    [Fact]
    public async Task Delayed_failed_save_does_not_replace_newer_success_or_end_its_busy_state()
    {
        var dispatcher = new DeferredUiDispatcher();
        var store = new PreferencesStore
        {
            Failure = new IOException("old error"),
            BeforeSave = () => { dispatcher.DeferNext = true; return Task.CompletedTask; },
        };
        var viewModel = new AppSettingsViewModel(new AppPreferencesManager(store), dispatcher);
        var failed = viewModel.SetThemeAsync(InterfaceTheme.Dark);
        await dispatcher.Entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
        store.Failure = null;
        store.BeforeSave = null;

        await viewModel.SetLanguageAsync(InterfaceLanguage.English);
        Assert.True(viewModel.IsSaving);
        dispatcher.Release();
        await failed;

        Assert.False(viewModel.IsSaving);
        Assert.False(viewModel.HasError);
        Assert.Equal(InterfaceLanguage.English, viewModel.Language);
        Assert.Equal(InterfaceTheme.System, viewModel.Theme);
    }

    [Fact]
    public async Task Saving_and_error_notifications_are_dispatched_to_the_ui()
    {
        var dispatcher = new DeferredUiDispatcher();
        var store = new PreferencesStore { Failure = new IOException("disk unavailable") };
        var viewModel = new AppSettingsViewModel(new AppPreferencesManager(store), dispatcher);
        viewModel.PropertyChanged += (_, _) => Assert.True(dispatcher.IsInvoking);

        await viewModel.SetThemeAsync(InterfaceTheme.Dark);
        store.Failure = null;
        await viewModel.SetThemeAsync(InterfaceTheme.Dark);
    }

    private sealed class DeferredUiDispatcher : IUiDispatcher
    {
        public bool DeferNext { get; set; }
        public bool IsInvoking { get; private set; }
        public TaskCompletionSource Entered { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        private Action? _deferred;
        private readonly TaskCompletionSource _completed = new(TaskCreationOptions.RunContinuationsAsynchronously);

        public Task InvokeAsync(Action action, CancellationToken cancellationToken = default)
        {
            if (DeferNext)
            {
                DeferNext = false;
                _deferred = action;
                Entered.TrySetResult();
                return _completed.Task;
            }
            IsInvoking = true;
            try { action(); }
            finally { IsInvoking = false; }
            return Task.CompletedTask;
        }

        public void Release()
        {
            InvokeAsync(_deferred!).GetAwaiter().GetResult();
            _completed.TrySetResult();
        }
    }

    private static Task ChangeAsync(AppSettingsViewModel viewModel, string setting) => setting switch
    {
        "language" => viewModel.SetLanguageAsync(InterfaceLanguage.English),
        "theme" => viewModel.SetThemeAsync(InterfaceTheme.Dark),
        "font" => viewModel.SetFontScaleAsync(1.2),
        "pet" => viewModel.SetPetEnabledAsync(false),
        "petSize" => viewModel.SetPetSizeAsync(150),
        "process" => viewModel.SetPetShowProcessAsync(false),
        "completion" => viewModel.SetPetShowCompletionsAsync(false),
        "utilities" => viewModel.SetGlobalUtilitiesEnabledAsync(false),
        "screenshot" => viewModel.SetGlobalScreenshotEnabledAsync(false),
        "recording" => viewModel.SetGlobalRecordingEnabledAsync(false),
        "clipboard" => viewModel.SetGlobalClipboardEnabledAsync(false),
        "search" => viewModel.SetGlobalQuickSearchEnabledAsync(false),
        _ => throw new ArgumentOutOfRangeException(nameof(setting)),
    };

    private sealed class PreferencesStore : IAppPreferencesStore
    {
        public AppPreferences Value { get; set; } = AppPreferences.Default;
        public Exception? Failure { get; set; }
        public int SaveCount { get; private set; }
        public Func<Task>? BeforeSave { get; set; }

        public Task<AppPreferences?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<AppPreferences?>(Value);

        public async Task SaveAsync(AppPreferences preferences, CancellationToken cancellationToken = default)
        {
            SaveCount++;
            if (BeforeSave is not null) await BeforeSave();
            cancellationToken.ThrowIfCancellationRequested();
            if (Failure is not null) throw Failure;
            Value = preferences;
        }
    }
}
