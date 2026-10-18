using System.Diagnostics;
using System.Text.Json;
using System.Threading.Channels;
using ChatOS.Connector.LocalAgent;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentHostLifecycleTests
{
    [Fact]
    public async Task UsesProtocol42AndCorrelatesConcurrentResponses()
    {
        await using var process = new FakeProcess();
        await using var lifecycle = new WindowsLocalAgentHostLifecycle(
            Options() with { RequestTimeout = TimeSpan.FromSeconds(2) },
            new FakeLauncher(process));
        await lifecycle.StartForOwnerAsync("owner-1");

        var slow = lifecycle.SendAsync<TestCommand, TestResult>(
            new("test", "owner-1", "slow"));
        var fast = lifecycle.SendAsync<TestCommand, TestResult>(
            new("test", "owner-1", "fast"));

        Assert.Same(fast, await Task.WhenAny(slow, fast));
        Assert.Equal("fast", (await fast).Value);
        Assert.Equal("slow", (await slow).Value);
        Assert.All(process.ProtocolVersions, value => Assert.Equal(42, value));
    }

    [Fact]
    public async Task DeadlineInvalidatesHungTransportAndRaisesRecoverySignal()
    {
        await using var process = new FakeProcess();
        await using var lifecycle = new WindowsLocalAgentHostLifecycle(
            Options() with { RequestTimeout = TimeSpan.FromMilliseconds(50) },
            new FakeLauncher(process));
        var exited = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        lifecycle.UnexpectedExit += (_, _) => exited.TrySetResult();
        await lifecycle.StartForOwnerAsync("owner-1");

        await Assert.ThrowsAsync<TimeoutException>(() =>
            lifecycle.SendAsync<TestCommand, TestResult>(new("test", "owner-1", "never")));

        await exited.Task.WaitAsync(TimeSpan.FromSeconds(1));
        Assert.Null(lifecycle.ActiveOwnerUserId);
    }

    [Fact]
    public void ConversationStartCommandUsesStableTurnScopedIdentity()
    {
        var first = JsonSerializer.SerializeToElement(new
        {
            type = "start_conversation_turn",
            turn_id = "turn-retry-1",
            message = "first",
        });
        var changedPayload = JsonSerializer.SerializeToElement(new
        {
            type = "start_conversation_turn",
            turn_id = "turn-retry-1",
            message = "second",
        });
        var ordinary = JsonSerializer.SerializeToElement(new
        {
            type = "get_conversation",
            conversation_id = "conversation-1",
        });

        Assert.Equal(
            WindowsLocalAgentHostLifecycle.CreateCommandId(first),
            WindowsLocalAgentHostLifecycle.CreateCommandId(changedPayload));
        Assert.NotEqual(
            WindowsLocalAgentHostLifecycle.CreateCommandId(ordinary),
            WindowsLocalAgentHostLifecycle.CreateCommandId(ordinary));
    }

    [Fact]
    public async Task DisposeIsIdempotent()
    {
        await using var process = new FakeProcess();
        var lifecycle = new WindowsLocalAgentHostLifecycle(
            Options(), new FakeLauncher(process));
        await lifecycle.StartForOwnerAsync("owner-1");

        await lifecycle.DisposeAsync();
        await lifecycle.DisposeAsync();

        Assert.Null(lifecycle.ActiveOwnerUserId);
    }

    [Fact]
    public async Task BootstrapDisablesWorkersAndIdenticalRefreshReusesActiveWorker()
    {
        var launcher = new SequenceLauncher();
        await using var lifecycle = new WindowsLocalAgentHostLifecycle(Options(), launcher);
        var credentials = new Dictionary<string, string>
        {
            ["CHATOS_LOCAL_AGENT_MODEL_TEST"] = "model-secret",
            ["CHATOS_MEMORY_ACCESS_TOKEN"] = "memory-secret",
        };

        await lifecycle.StartForBootstrapAsync("owner-1");
        Assert.False(launcher.Launches.Single().WorkersEnabled);
        var bootstrap = launcher.Launches.Single().Process;

        await lifecycle.RefreshCredentialsForOwnerAsync("owner-1", credentials);
        Assert.True(bootstrap.HasExited);
        Assert.Equal(2, launcher.Launches.Count);
        Assert.True(launcher.Launches[1].WorkersEnabled);
        var worker = launcher.Launches[1].Process;

        await lifecycle.RefreshCredentialsForOwnerAsync(
            "owner-1",
            new Dictionary<string, string>
            {
                ["CHATOS_MEMORY_ACCESS_TOKEN"] = "memory-secret",
                ["CHATOS_LOCAL_AGENT_MODEL_TEST"] = "model-secret",
            });

        Assert.Equal(2, launcher.Launches.Count);
        Assert.False(worker.HasExited);
    }

    [Fact]
    public async Task ChangedCredentialRefreshIsDeferredWithoutReplacingWorker()
    {
        var launcher = new SequenceLauncher();
        await using var lifecycle = new WindowsLocalAgentHostLifecycle(Options(), launcher);
        await lifecycle.RefreshCredentialsForOwnerAsync(
            "owner-1",
            new Dictionary<string, string>
            {
                ["CHATOS_LOCAL_AGENT_MODEL_TEST"] = "original-secret",
            });
        var worker = launcher.Launches.Single().Process;

        await Assert.ThrowsAsync<LocalAgentCredentialRefreshDeferredException>(() =>
            lifecycle.RefreshCredentialsForOwnerAsync(
                "owner-1",
                new Dictionary<string, string>
                {
                    ["CHATOS_LOCAL_AGENT_MODEL_TEST"] = "rotated-secret",
                }));
        await Assert.ThrowsAsync<InvalidOperationException>(() =>
            lifecycle.RefreshCredentialsForOwnerAsync(
                "owner-2",
                new Dictionary<string, string>
                {
                    ["CHATOS_LOCAL_AGENT_MODEL_TEST"] = "original-secret",
                }));

        Assert.Single(launcher.Launches);
        Assert.False(worker.HasExited);
        Assert.DoesNotContain("original-secret", lifecycle.ToString(), StringComparison.Ordinal);

        await lifecycle.StopAsync();
        await lifecycle.RefreshCredentialsForOwnerAsync(
            "owner-1",
            new Dictionary<string, string>
            {
                ["CHATOS_LOCAL_AGENT_MODEL_TEST"] = "rotated-secret",
            });

        Assert.Equal(2, launcher.Launches.Count);
        Assert.True(worker.HasExited);
        Assert.True(launcher.Launches[1].WorkersEnabled);
    }

    private static LocalAgentHostOptions Options() => new(
        "host.exe",
        Path.Combine(Path.GetTempPath(), $"local-agent-{Guid.NewGuid():N}.sqlite3"),
        TimeSpan.FromSeconds(1),
        new Uri("https://example.test/api/memory"),
        "local_agent",
        TimeSpan.FromSeconds(1));

    private sealed record TestCommand(string Type, string OwnerUserId, string Value);
    private sealed record TestResult(string Type, string Value);

    private sealed class FakeLauncher(FakeProcess process) : ILocalAgentHostProcessLauncher
    {
        public Task<ILocalAgentHostProcess> LaunchAsync(
            LocalAgentHostOptions options,
            string ownerUserId,
            IReadOnlyDictionary<string, string> credentialEnvironment,
            bool workersEnabled,
            CancellationToken cancellationToken) => Task.FromResult<ILocalAgentHostProcess>(process);
    }

    private sealed class SequenceLauncher : ILocalAgentHostProcessLauncher
    {
        internal List<LaunchRecord> Launches { get; } = [];

        public Task<ILocalAgentHostProcess> LaunchAsync(
            LocalAgentHostOptions options,
            string ownerUserId,
            IReadOnlyDictionary<string, string> credentialEnvironment,
            bool workersEnabled,
            CancellationToken cancellationToken)
        {
            var process = new FakeProcess();
            Launches.Add(new LaunchRecord(process, workersEnabled));
            return Task.FromResult<ILocalAgentHostProcess>(process);
        }
    }

    private sealed record LaunchRecord(FakeProcess Process, bool WorkersEnabled);

    private sealed class FakeProcess : ILocalAgentHostProcess
    {
        private readonly InMemoryPipeStream _requests = new();
        private readonly InMemoryPipeStream _responses = new();
        private readonly SemaphoreSlim _responseGate = new(1, 1);
        private readonly CancellationTokenSource _lifetime = new();
        private readonly Task _responder;
        private int _disposed;

        internal FakeProcess()
        {
            _responder = Task.Run(RespondAsync);
        }

        public event EventHandler? Exited;
        public Stream StandardInput => _requests;
        public Stream StandardOutput => _responses;
        public bool HasExited { get; private set; }
        internal System.Collections.Concurrent.ConcurrentQueue<int> ProtocolVersions { get; } = [];

        private async Task RespondAsync()
        {
            try
            {
                while (!_lifetime.IsCancellationRequested)
                {
                    var payload = await LocalAgentHostFrameCodec.ReadAsync(
                        _requests, _lifetime.Token).ConfigureAwait(false);
                    using var document = JsonDocument.Parse(payload);
                    var root = document.RootElement;
                    var protocol = root.GetProperty("protocol_version").GetInt32();
                    var commandId = root.GetProperty("command_id").GetString()!;
                    var command = root.GetProperty("command");
                    var type = command.GetProperty("type").GetString();
                    ProtocolVersions.Enqueue(protocol);
                    if (type == "health")
                    {
                        await WriteAsync(commandId, new { type = "health", storage_ready = true }, 0)
                            .ConfigureAwait(false);
                        continue;
                    }
                    var value = command.GetProperty("value").GetString()!;
                    if (value == "never") continue;
                    _ = Task.Run(() => WriteAsync(
                        commandId, new { type = "test", value }, value == "slow" ? 150 : 10));
                }
            }
            catch (OperationCanceledException)
            {
            }
            catch (EndOfStreamException)
            {
            }
        }

        private async Task WriteAsync(string commandId, object result, int delayMilliseconds)
        {
            if (delayMilliseconds > 0)
                await Task.Delay(delayMilliseconds, _lifetime.Token).ConfigureAwait(false);
            var payload = JsonSerializer.SerializeToUtf8Bytes(new
            {
                protocol_version = 42,
                command_id = commandId,
                ok = true,
                result,
                error = (object?)null,
            });
            await _responseGate.WaitAsync(_lifetime.Token).ConfigureAwait(false);
            try
            {
                await LocalAgentHostFrameCodec.WriteAsync(
                    _responses, payload, _lifetime.Token).ConfigureAwait(false);
            }
            finally
            {
                _responseGate.Release();
            }
        }

        public Task TerminateAsync()
        {
            if (HasExited) return Task.CompletedTask;
            HasExited = true;
            _lifetime.Cancel();
            _requests.Complete();
            _responses.Complete();
            Exited?.Invoke(this, EventArgs.Empty);
            return Task.CompletedTask;
        }

        public async ValueTask DisposeAsync()
        {
            if (Interlocked.Exchange(ref _disposed, 1) != 0) return;
            await TerminateAsync().ConfigureAwait(false);
            try { await _responder.ConfigureAwait(false); }
            catch (OperationCanceledException) { }
            _lifetime.Dispose();
            _responseGate.Dispose();
        }
    }

    private sealed class InMemoryPipeStream : Stream
    {
        private readonly Channel<byte[]> _chunks = Channel.CreateUnbounded<byte[]>(
            new UnboundedChannelOptions { SingleReader = true, SingleWriter = false });
        private byte[]? _current;
        private int _offset;

        public override bool CanRead => true;
        public override bool CanSeek => false;
        public override bool CanWrite => true;
        public override long Length => throw new NotSupportedException();
        public override long Position { get => throw new NotSupportedException(); set => throw new NotSupportedException(); }

        public void Complete() => _chunks.Writer.TryComplete();
        public override void Flush() { }
        public override Task FlushAsync(CancellationToken cancellationToken) => Task.CompletedTask;

        public override async ValueTask WriteAsync(
            ReadOnlyMemory<byte> buffer,
            CancellationToken cancellationToken = default)
        {
            await _chunks.Writer.WriteAsync(buffer.ToArray(), cancellationToken).ConfigureAwait(false);
        }

        public override void Write(byte[] buffer, int offset, int count) =>
            _chunks.Writer.TryWrite(buffer.AsSpan(offset, count).ToArray());

        public override async ValueTask<int> ReadAsync(
            Memory<byte> buffer,
            CancellationToken cancellationToken = default)
        {
            while (_current is null || _offset == _current.Length)
            {
                if (!await _chunks.Reader.WaitToReadAsync(cancellationToken).ConfigureAwait(false))
                    return 0;
                if (!_chunks.Reader.TryRead(out _current)) continue;
                _offset = 0;
            }
            var count = Math.Min(buffer.Length, _current.Length - _offset);
            _current.AsMemory(_offset, count).CopyTo(buffer);
            _offset += count;
            return count;
        }

        public override int Read(byte[] buffer, int offset, int count) =>
            ReadAsync(buffer.AsMemory(offset, count)).AsTask().GetAwaiter().GetResult();
        public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();
        public override void SetLength(long value) => throw new NotSupportedException();
    }
}
