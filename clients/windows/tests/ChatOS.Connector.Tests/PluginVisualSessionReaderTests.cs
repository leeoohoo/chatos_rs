using System.Text.Json;
using ChatOS.Connector.Plugins;

namespace ChatOS.Connector.Tests;

public sealed class PluginVisualSessionReaderTests : IDisposable
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(),
        $"chatos-plugin-visual-{Guid.NewGuid():N}");

    [Fact]
    public async Task KeepsMultipleOwnedSessionsAndLoadsOnlySelectedFrameBytes()
    {
        var store = new PluginRuntimeSessionStore();
        var firstFrame = new byte[] { 1, 2, 3, 4 };
        var secondFrame = new byte[] { 5, 6, 7, 8 };
        await AddSessionAsync(store, "adapter-a", "browser", firstFrame, "2020-01-01T00:00:00Z");
        await AddSessionAsync(store, "adapter-b", "computer-use", secondFrame, "2020-01-01T00:00:00Z");
        store.BindVisualOwner("adapter-a", new PluginVisualSessionOwner(
            "conversation-1", TaskRunId: "task-run-a", TaskTitle: "检查网站"));
        store.BindVisualOwner("adapter-b", new PluginVisualSessionOwner(
            "conversation-1", TaskRunId: "task-run-b", TaskTitle: "整理桌面"));
        var reader = new PluginVisualSessionReader(store);

        var sessions = await reader.ReadAsync(new HashSet<string> { "adapter-a" });

        Assert.Equal(2, sessions.Count);
        var first = Assert.Single(sessions, value => value.AdapterSessionId == "adapter-a");
        var second = Assert.Single(sessions, value => value.AdapterSessionId == "adapter-b");
        Assert.Equal(firstFrame, first.FrameData);
        Assert.Null(second.FrameData);
        Assert.Equal("conversation-1", first.Owner.ConversationId);
        Assert.Equal("检查网站", first.Owner.TaskTitle);
        Assert.Equal(7UL, first.FrameSequence);
        Assert.Equal("Open browser", first.PluginDisplayName);
    }

    [Fact]
    public async Task RejectsMismatchedHostAndRemovesSessionAfterFullCancellation()
    {
        var store = new PluginRuntimeSessionStore();
        await AddSessionAsync(store, "adapter-safe", "computer-use", [9, 8, 7], DateTimeOffset.UtcNow.ToString("O"));
        store.BindVisualOwner("adapter-safe", new PluginVisualSessionOwner("conversation-1"));
        var root = VisualRoot("adapter-safe");
        await File.WriteAllTextAsync(
            Path.Combine(root, "host.json"),
            """{"protocol_version":1,"adapter_session_id":"other","plugin_id":"plugin-1","component_key":"computer-use"}""");
        var reader = new PluginVisualSessionReader(store);

        Assert.Empty(await reader.ReadAsync());

        await WriteHostAsync(root, "adapter-safe", "computer-use");
        Assert.Single(await reader.ReadAsync());
        Assert.Equal(
            "cancelled",
            await store.CancelAsync("adapter-safe", null, "workspace-1"));
        Assert.Empty(await reader.ReadAsync());
    }

    public void Dispose()
    {
        try
        {
            if (Directory.Exists(_directory))
            {
                Directory.Delete(_directory, recursive: true);
            }
        }
        catch (IOException)
        {
        }
    }

    private async Task AddSessionAsync(
        PluginRuntimeSessionStore store,
        string adapterSessionId,
        string componentKey,
        byte[] frame,
        string capturedAt)
    {
        var root = VisualRoot(adapterSessionId);
        Directory.CreateDirectory(root);
        await WriteHostAsync(root, adapterSessionId, componentKey);
        await File.WriteAllBytesAsync(Path.Combine(root, "frame.png"), frame);
        await File.WriteAllTextAsync(
            Path.Combine(root, "session.json"),
            JsonSerializer.Serialize(new
            {
                protocol_version = 1,
                session_id = $"visual-{adapterSessionId}",
                status = "running",
                title = "实时操作",
                target_app = "Notes",
                mime_type = "image/png",
                frame_file = "frame.png",
                frame_sequence = 7,
                captured_at = capturedAt,
                width = 960,
                height = 600,
            }));
        await store.InsertAsync(
            new PluginRuntimeIdentity(
                $"run-{adapterSessionId}",
                "plugin-1",
                "release-1",
                "1.0.0",
                new string('a', 64),
                componentKey,
                adapterSessionId,
                "workspace-1"),
            new FakeClient(),
            [JsonSerializer.SerializeToElement(new { name = "echo" })],
            new HashSet<string>(),
            requiresExclusiveExecution: false,
            _directory,
            Path.Combine(_directory, "artifacts", adapterSessionId),
            root,
            componentKey == "browser" ? "Open browser" : "Open computer use");
    }

    private static Task WriteHostAsync(string root, string adapterSessionId, string componentKey) =>
        File.WriteAllTextAsync(
            Path.Combine(root, "host.json"),
            JsonSerializer.Serialize(new
            {
                protocol_version = 1,
                adapter_session_id = adapterSessionId,
                plugin_id = "plugin-1",
                component_key = componentKey,
            }));

    private string VisualRoot(string adapterSessionId) =>
        Path.Combine(_directory, "visual", adapterSessionId);

    private sealed class FakeClient : IPluginMcpClient
    {
        public Task StartAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task<PluginMcpInitialization> InitializeAsync(CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
        public Task<JsonElement> CallToolAsync(
            string name,
            JsonElement arguments,
            TimeSpan timeout,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task TerminateAsync() => Task.CompletedTask;
        public ValueTask DisposeAsync() => ValueTask.CompletedTask;
    }
}
