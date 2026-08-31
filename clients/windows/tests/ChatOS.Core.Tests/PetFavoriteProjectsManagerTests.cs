using ChatOS.Core.Abstractions;
using ChatOS.Core.State;

namespace ChatOS.Core.Tests;

public sealed class PetFavoriteProjectsManagerTests
{
    [Fact]
    public async Task Initializes_normalized_ids_and_persists_atomic_changes()
    {
        var store = new MemoryStore([" project-two ", "project-one", "project-one"]);
        var manager = new PetFavoriteProjectsManager(store);
        var changed = 0;
        manager.Changed += (_, _) => changed++;

        await manager.InitializeAsync();
        await manager.SetFavoriteAsync("project-three", true);
        await manager.SetFavoriteAsync("project-one", false);

        Assert.False(manager.IsFavorite("project-one"));
        Assert.True(manager.IsFavorite("project-two"));
        Assert.True(manager.IsFavorite("project-three"));
        Assert.Equal(2, changed);
        Assert.Equal(["project-three", "project-two"], store.Saved);
    }

    [Fact]
    public async Task Store_failure_does_not_publish_an_unpersisted_favorite()
    {
        var store = new MemoryStore([]) { SaveError = new IOException("disk full") };
        var manager = new PetFavoriteProjectsManager(store);
        await manager.InitializeAsync();

        await Assert.ThrowsAsync<IOException>(() => manager.SetFavoriteAsync("project-one", true));

        Assert.False(manager.IsFavorite("project-one"));
    }

    private sealed class MemoryStore(IReadOnlyList<string> initial) : IPetFavoriteProjectsStore
    {
        public Exception? SaveError { get; set; }
        public IReadOnlyList<string> Saved { get; private set; } = initial;

        public Task<IReadOnlyList<string>> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(Saved);

        public Task SaveAsync(
            IReadOnlyCollection<string> projectIds,
            CancellationToken cancellationToken = default)
        {
            if (SaveError is not null) return Task.FromException(SaveError);
            Saved = projectIds.ToArray();
            return Task.CompletedTask;
        }
    }
}
