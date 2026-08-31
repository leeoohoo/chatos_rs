using System.Text;
using ChatOS.Connector.Plugins;
using ChatOS.Desktop.Features.Plugins;
using ChatOS.Presentation.Threading;

namespace ChatOS.Connector.Tests;

public sealed class PluginArtifactsViewModelTests : IDisposable
{
    private readonly string _root = Path.Combine(
        Path.GetTempPath(),
        $"chatos-artifact-vm-{Guid.NewGuid():N}");

    [Fact]
    public async Task LoadsArtifactsForRequestedAdapterSession()
    {
        var service = new FakeArtifactService(Descriptor("artifact-a", "adapter-a"));
        var interaction = new FakeInteraction(_root);
        var viewModel = new PluginArtifactsViewModel(service, interaction, new ImmediateUiDispatcher());

        await viewModel.LoadAsync("adapter-a");

        Assert.Equal("adapter-a", service.LastAdapterSessionId);
        Assert.Single(viewModel.Artifacts);
    }

    [Fact]
    public async Task CancelledSavePickerDoesNotCopyArtifact()
    {
        var service = new FakeArtifactService(Descriptor("artifact-a", "adapter-a"));
        var interaction = new FakeInteraction(_root) { SavePath = null };
        var viewModel = new PluginArtifactsViewModel(service, interaction, new ImmediateUiDispatcher());
        await viewModel.LoadAsync();

        await viewModel.SaveAsAsync(viewModel.Artifacts[0]);

        Assert.Equal(0, service.CopyCount);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task SaveUsesTemporaryFileAndAtomicallyReplacesTarget()
    {
        Directory.CreateDirectory(_root);
        var target = Path.Combine(_root, "result.txt");
        await File.WriteAllTextAsync(target, "old");
        var service = new FakeArtifactService(Descriptor("artifact-a", "adapter-a"));
        var interaction = new FakeInteraction(_root) { SavePath = target };
        var viewModel = new PluginArtifactsViewModel(service, interaction, new ImmediateUiDispatcher());
        await viewModel.LoadAsync();

        await viewModel.SaveAsAsync(viewModel.Artifacts[0]);

        Assert.Equal("artifact bytes", await File.ReadAllTextAsync(target));
        Assert.Empty(Directory.EnumerateFiles(_root, "*.chatos.tmp"));
    }

    [Fact]
    public async Task FailedCopyDeletesTemporaryFileAndPreservesExistingTarget()
    {
        Directory.CreateDirectory(_root);
        var target = Path.Combine(_root, "result.txt");
        await File.WriteAllTextAsync(target, "existing");
        var service = new FakeArtifactService(Descriptor("artifact-a", "adapter-a"))
        {
            CopyError = new IOException("copy failed"),
        };
        var interaction = new FakeInteraction(_root) { SavePath = target };
        var viewModel = new PluginArtifactsViewModel(service, interaction, new ImmediateUiDispatcher());
        await viewModel.LoadAsync();

        await viewModel.SaveAsAsync(viewModel.Artifacts[0]);

        Assert.Equal("existing", await File.ReadAllTextAsync(target));
        Assert.Equal("copy failed", viewModel.ErrorMessage);
        Assert.Empty(Directory.EnumerateFiles(_root, "*.chatos.tmp"));
    }

    [Fact]
    public async Task OpenCopiesToControlledCacheWithSanitizedName()
    {
        var service = new FakeArtifactService(Descriptor("artifact-a", "adapter-a", "report?.txt"));
        var interaction = new FakeInteraction(_root);
        var viewModel = new PluginArtifactsViewModel(service, interaction, new ImmediateUiDispatcher());
        await viewModel.LoadAsync();

        await viewModel.OpenAsync(viewModel.Artifacts[0]);

        Assert.NotNull(interaction.OpenedPath);
        Assert.Equal(Path.GetFullPath(_root), Path.GetDirectoryName(interaction.OpenedPath));
        Assert.True(File.Exists(interaction.OpenedPath));
        Assert.DoesNotContain('?', Path.GetFileName(interaction.OpenedPath));
    }

    public void Dispose()
    {
        if (Directory.Exists(_root)) Directory.Delete(_root, true);
    }

    private static PluginArtifactDescriptor Descriptor(
        string id,
        string adapter,
        string name = "result.txt") => new(
        id,
        new PluginArtifactOwner(
            "user", "run", "device", "workspace", "plugin", "release", new string('a', 64), "component", adapter),
        name,
        name,
        "text/plain",
        14,
        new string('b', 64),
        DateTimeOffset.UnixEpoch,
        "tool",
        true,
        false);

    private sealed class FakeArtifactService(params PluginArtifactDescriptor[] descriptors) : IPluginArtifactService
    {
        public string? LastAdapterSessionId { get; private set; }
        public int CopyCount { get; private set; }
        public Exception? CopyError { get; set; }

        public Task<IReadOnlyList<PluginArtifactDescriptor>> ListAsync(
            string? adapterSessionId = null,
            CancellationToken cancellationToken = default)
        {
            LastAdapterSessionId = adapterSessionId;
            IReadOnlyList<PluginArtifactDescriptor> values = descriptors
                .Where(value => adapterSessionId is null || value.Owner.AdapterSessionId == adapterSessionId)
                .ToArray();
            return Task.FromResult(values);
        }

        public async Task<PluginArtifactDescriptor> CopyToAsync(
            string artifactId,
            Stream destination,
            CancellationToken cancellationToken = default)
        {
            CopyCount++;
            if (CopyError is not null) throw CopyError;
            await destination.WriteAsync(Encoding.UTF8.GetBytes("artifact bytes"), cancellationToken);
            return descriptors.Single(value => value.ArtifactId == artifactId);
        }
    }

    private sealed class FakeInteraction(string cacheDirectory) : IPluginArtifactUserInteraction
    {
        public string CacheDirectory { get; } = cacheDirectory;
        public string? SavePath { get; set; }
        public string? OpenedPath { get; private set; }

        public Task<string?> PickSavePathAsync(
            string suggestedFileName,
            CancellationToken cancellationToken = default) => Task.FromResult(SavePath);

        public Task OpenFileAsync(string path, CancellationToken cancellationToken = default)
        {
            OpenedPath = path;
            return Task.CompletedTask;
        }
    }
}
