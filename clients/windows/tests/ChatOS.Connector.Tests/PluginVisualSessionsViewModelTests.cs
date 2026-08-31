using ChatOS.Connector.Plugins;
using ChatOS.Desktop.Features.Plugins;
using ChatOS.Presentation.Threading;

namespace ChatOS.Connector.Tests;

public sealed class PluginVisualSessionsViewModelTests
{
    [Fact]
    public async Task LoadsFrameOnlyForSelectedAdapterSession()
    {
        var service = new FakeVisualService(
            Session("adapter-a", 1, null),
            Session("adapter-b", 2, null));
        var viewModel = new PluginVisualSessionsViewModel(service, new ImmediateUiDispatcher());

        await viewModel.RefreshAsync();

        Assert.Equal(2, service.Requests.Count);
        Assert.Empty(service.Requests[0]);
        Assert.Equal(["adapter-a"], service.Requests[1]);
        Assert.Equal("adapter-a", viewModel.SelectedSession?.AdapterSessionId);
        Assert.NotNull(viewModel.SelectedSession?.FrameData);
        Assert.Null(viewModel.Sessions.Single(value => value.AdapterSessionId == "adapter-b").FrameData);

        await viewModel.SelectAsync("adapter-b");

        Assert.Equal("adapter-b", viewModel.SelectedSession?.AdapterSessionId);
        Assert.NotNull(viewModel.SelectedSession?.FrameData);
        Assert.Equal(["adapter-b"], service.Requests[^1]);
    }

    [Fact]
    public async Task DismissedSessionDoesNotReappearUntilSourceEnds()
    {
        var service = new FakeVisualService(Session("adapter-a", 1, null));
        var viewModel = new PluginVisualSessionsViewModel(service, new ImmediateUiDispatcher());
        await viewModel.RefreshAsync();

        await viewModel.DismissSelectedAsync();
        await viewModel.RefreshAsync();

        Assert.Empty(viewModel.Sessions);

        service.Sessions = [];
        await viewModel.RefreshAsync();
        service.Sessions = [Session("adapter-a", 2, null)];
        await viewModel.RefreshAsync();

        Assert.Single(viewModel.Sessions);
    }

    [Fact]
    public async Task NewerRefreshWinsWhenOlderReadCompletesLate()
    {
        var service = new RacingVisualService();
        var viewModel = new PluginVisualSessionsViewModel(service, new ImmediateUiDispatcher());

        var older = viewModel.RefreshAsync();
        await service.FirstReadStarted.Task;
        var newer = viewModel.RefreshAsync();
        await newer;
        service.ReleaseFirstRead.SetResult();
        await older;

        Assert.Equal("new", viewModel.SelectedSession?.Id);
    }

    [Fact]
    public async Task ReadFailureIsExposedWithoutDiscardingCurrentFrame()
    {
        var service = new FakeVisualService(Session("adapter-a", 1, null));
        var viewModel = new PluginVisualSessionsViewModel(service, new ImmediateUiDispatcher());
        await viewModel.RefreshAsync();
        service.Error = new InvalidOperationException("visual failed");

        await viewModel.RefreshAsync();

        Assert.Equal("visual failed", viewModel.ErrorMessage);
        Assert.Single(viewModel.Sessions);
    }

    private static PluginVisualSession Session(string adapter, ulong sequence, byte[]? frame) => new(
        $"session-{sequence}",
        adapter,
        "plugin",
        "component",
        "Plugin",
        $"Frame {sequence}",
        "Browser",
        sequence,
        DateTimeOffset.UnixEpoch.AddSeconds(sequence),
        frame,
        "image/png",
        800,
        600,
        new PluginVisualSessionOwner("conversation"));

    private sealed class FakeVisualService(params PluginVisualSession[] sessions) : IPluginVisualSessionService
    {
        public IReadOnlyList<PluginVisualSession> Sessions { get; set; } = sessions;
        public Exception? Error { get; set; }
        public List<string[]> Requests { get; } = [];

        public Task<IReadOnlyList<PluginVisualSession>> ReadAsync(
            IReadOnlySet<string>? loadFrameDataForAdapterSessionIds = null,
            CancellationToken cancellationToken = default)
        {
            if (Error is not null) throw Error;
            var requested = loadFrameDataForAdapterSessionIds?.OrderBy(value => value).ToArray() ?? [];
            Requests.Add(requested);
            IReadOnlyList<PluginVisualSession> values = Sessions.Select(value => value with
            {
                FrameData = requested.Contains(value.AdapterSessionId) ? [1, 2, 3] : null,
            }).ToArray();
            return Task.FromResult(values);
        }
    }

    private sealed class RacingVisualService : IPluginVisualSessionService
    {
        private int _calls;
        public TaskCompletionSource FirstReadStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource ReleaseFirstRead { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);

        public async Task<IReadOnlyList<PluginVisualSession>> ReadAsync(
            IReadOnlySet<string>? loadFrameDataForAdapterSessionIds = null,
            CancellationToken cancellationToken = default)
        {
            var call = Interlocked.Increment(ref _calls);
            if (call == 1)
            {
                FirstReadStarted.SetResult();
                await ReleaseFirstRead.Task;
                return [Session("old-adapter", 1, null) with { Id = "old" }];
            }

            var frame = loadFrameDataForAdapterSessionIds is { Count: > 0 } ? new byte[] { 1 } : null;
            return [Session("new-adapter", 2, frame) with { Id = "new" }];
        }
    }
}
