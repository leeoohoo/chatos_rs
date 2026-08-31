using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;

namespace ChatOS.Core.Tests;

public sealed class AppPreferencesManagerTests
{
    [Fact]
    public async Task InitializeUsesNormalizedStoredPreferences()
    {
        var store = new MemoryPreferencesStore(new AppPreferences(
            InterfaceLanguage.English,
            InterfaceTheme.Dark,
            9,
            false));
        var manager = new AppPreferencesManager(store);

        await manager.InitializeAsync();

        Assert.Equal(InterfaceLanguage.English, manager.Current.Language);
        Assert.Equal(InterfaceTheme.Dark, manager.Current.Theme);
        Assert.Equal(AppPreferences.MaximumFontScale, manager.Current.FontScale);
        Assert.False(manager.Current.PetEnabled);
    }

    [Fact]
    public async Task UpdatePersistsBeforePublishingChange()
    {
        var store = new MemoryPreferencesStore(null);
        var manager = new AppPreferencesManager(store);
        await manager.InitializeAsync();
        AppPreferences? observed = null;
        manager.Changed += (_, value) => observed = value;

        await manager.UpdateAsync(value => value with { Language = InterfaceLanguage.English });

        Assert.Equal(InterfaceLanguage.English, store.Value?.Language);
        Assert.Equal(store.Value, observed);
        Assert.Equal(store.Value, manager.Current);
    }

    private sealed class MemoryPreferencesStore(AppPreferences? value) : IAppPreferencesStore
    {
        public AppPreferences? Value { get; private set; } = value;

        public Task<AppPreferences?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(Value);

        public Task SaveAsync(AppPreferences preferences, CancellationToken cancellationToken = default)
        {
            Value = preferences;
            return Task.CompletedTask;
        }
    }
}
