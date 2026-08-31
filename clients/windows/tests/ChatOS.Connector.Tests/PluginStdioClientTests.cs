using System.Collections.Concurrent;
using System.Text;
using System.Text.Json;
using System.Threading.Channels;
using ChatOS.Connector.Plugins;

namespace ChatOS.Connector.Tests;

public sealed class PluginStdioClientTests
{
    [Fact]
    public async Task InitializesListsToolsAndCallsToolOverJsonLines()
    {
        var process = new FakePluginProcess();
        await using var client = new PluginStdioClient(Launch(), new FakeLauncher(process));
        await client.StartAsync();

        var initializeTask = client.InitializeAsync();
        var initialize = JsonDocument.Parse(await process.Input.ReadLineAsync());
        Assert.Equal("initialize", initialize.RootElement.GetProperty("method").GetString());
        process.Output.Feed(JsonSerializer.Serialize(new
        {
            jsonrpc = "2.0",
            id = initialize.RootElement.GetProperty("id").GetInt64(),
            result = new { instructions = "Use safely" },
        }) + "\n");

        var initializedNotification = JsonDocument.Parse(await process.Input.ReadLineAsync());
        Assert.Equal("notifications/initialized", initializedNotification.RootElement.GetProperty("method").GetString());
        var list = JsonDocument.Parse(await process.Input.ReadLineAsync());
        Assert.Equal("tools/list", list.RootElement.GetProperty("method").GetString());
        process.Output.Feed(JsonSerializer.Serialize(new
        {
            jsonrpc = "2.0",
            id = list.RootElement.GetProperty("id").GetInt64(),
            result = new
            {
                tools = new[] { new { name = "echo", inputSchema = new { type = "object" } } },
            },
        }) + "\n");

        var initialized = await initializeTask;
        Assert.Equal("Use safely", initialized.Instructions);
        Assert.Equal("echo", Assert.Single(initialized.Tools).GetProperty("name").GetString());

        var callTask = client.CallToolAsync(
            "echo",
            JsonSerializer.SerializeToElement(new { value = "hello" }),
            TimeSpan.FromSeconds(2));
        var call = JsonDocument.Parse(await process.Input.ReadLineAsync());
        Assert.Equal("tools/call", call.RootElement.GetProperty("method").GetString());
        process.Output.Feed(JsonSerializer.Serialize(new
        {
            jsonrpc = "2.0",
            id = call.RootElement.GetProperty("id").GetInt64(),
            result = new
            {
                content = new[] { new { type = "text", text = "hello" } },
            },
        }) + "\n");

        var result = await callTask;
        Assert.Equal("hello", result.GetProperty("content")[0].GetProperty("text").GetString());
    }

    [Fact]
    public async Task TimeoutTerminatesWholePluginProcess()
    {
        var process = new FakePluginProcess();
        await using var client = new PluginStdioClient(Launch(), new FakeLauncher(process));
        await client.StartAsync();

        await Assert.ThrowsAsync<PluginRuntimeException>(() => client.CallToolAsync(
            "blocked",
            JsonSerializer.SerializeToElement(new { }),
            TimeSpan.FromMilliseconds(30)));

        Assert.True(process.Terminated);
    }

    [Fact]
    public async Task OversizedOrInvalidResponseTerminatesSession()
    {
        var process = new FakePluginProcess();
        await using var client = new PluginStdioClient(Launch(), new FakeLauncher(process));
        await client.StartAsync();
        var call = client.CallToolAsync(
            "oversized",
            JsonSerializer.SerializeToElement(new { }),
            TimeSpan.FromSeconds(5));
        _ = await process.Input.ReadLineAsync();
        process.Output.Feed(new byte[PluginStdioClient.MaximumMessageBytes + 1]);

        await Assert.ThrowsAsync<PluginRuntimeException>(() => call);
        Assert.True(process.Terminated);
    }

    [Fact]
    public void RedactsSecretsFromStderrTail()
    {
        var value = PluginStdioClient.RedactErrorTail(
            Encoding.UTF8.GetBytes("Authorization: Bearer abc.def token=super-secret password=hunter2"));

        Assert.DoesNotContain("abc.def", value, StringComparison.Ordinal);
        Assert.DoesNotContain("super-secret", value, StringComparison.Ordinal);
        Assert.DoesNotContain("hunter2", value, StringComparison.Ordinal);
        Assert.Contains("<redacted>", value, StringComparison.Ordinal);
    }

    private static PreparedPluginLaunch Launch() => new(
        new InstalledPluginRecord(
            "plugin-1",
            "release-1",
            "1.0.0",
            new string('a', 64),
            "C:\\Plugin",
            DateTimeOffset.UtcNow,
            ["process.spawn"]),
        "main",
        new PluginMcpServer { Type = "stdio", Bin = "plugin.exe" },
        "C:\\Plugin\\plugin.exe",
        Array.Empty<string>(),
        new Dictionary<string, string>(),
        "C:\\Plugin",
        "C:\\Runtime\\Visual",
        "C:\\Runtime\\Artifacts",
        "Test Plugin");

    private sealed class FakeLauncher(FakePluginProcess process) : IPluginProcessLauncher
    {
        public Task<IPluginProcess> LaunchAsync(
            PreparedPluginLaunch launch,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IPluginProcess>(process);
    }

    private sealed class FakePluginProcess : IPluginProcess
    {
        private readonly TaskCompletionSource<int> _exit =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public CaptureStream Input { get; } = new();

        public FeedStream Output { get; } = new();

        public FeedStream Error { get; } = new();

        public bool Terminated { get; private set; }

        Stream IPluginProcess.StandardInput => Input;

        Stream IPluginProcess.StandardOutput => Output;

        Stream IPluginProcess.StandardError => Error;

        public bool HasExited => _exit.Task.IsCompleted;

        public Task<int> WaitForExitAsync(CancellationToken cancellationToken = default) =>
            _exit.Task.WaitAsync(cancellationToken);

        public Task TerminateAsync()
        {
            Terminated = true;
            Output.Complete();
            Error.Complete();
            _exit.TrySetResult(1);
            return Task.CompletedTask;
        }

        public async ValueTask DisposeAsync()
        {
            await TerminateAsync();
        }
    }

    private sealed class CaptureStream : Stream
    {
        private readonly Channel<byte[]> _writes = Channel.CreateUnbounded<byte[]>();
        private readonly List<byte> _buffer = [];

        public async Task<string> ReadLineAsync()
        {
            while (true)
            {
                var newline = _buffer.IndexOf((byte)'\n');
                if (newline >= 0)
                {
                    var line = Encoding.UTF8.GetString(_buffer.Take(newline).ToArray());
                    _buffer.RemoveRange(0, newline + 1);
                    return line;
                }

                _buffer.AddRange(await _writes.Reader.ReadAsync());
            }
        }

        public override void Write(byte[] buffer, int offset, int count) =>
            _writes.Writer.TryWrite(buffer.AsSpan(offset, count).ToArray());

        public override ValueTask WriteAsync(
            ReadOnlyMemory<byte> buffer,
            CancellationToken cancellationToken = default)
        {
            _writes.Writer.TryWrite(buffer.ToArray());
            return ValueTask.CompletedTask;
        }

        public override bool CanRead => false;
        public override bool CanSeek => false;
        public override bool CanWrite => true;
        public override long Length => throw new NotSupportedException();
        public override long Position { get => throw new NotSupportedException(); set => throw new NotSupportedException(); }
        public override void Flush() { }
        public override Task FlushAsync(CancellationToken cancellationToken) => Task.CompletedTask;
        public override int Read(byte[] buffer, int offset, int count) => throw new NotSupportedException();
        public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();
        public override void SetLength(long value) => throw new NotSupportedException();
    }

    private sealed class FeedStream : Stream
    {
        private readonly Channel<byte[]> _chunks = Channel.CreateUnbounded<byte[]>();
        private byte[]? _current;
        private int _offset;

        public void Feed(string value) => Feed(Encoding.UTF8.GetBytes(value));

        public void Feed(byte[] value) => _chunks.Writer.TryWrite(value);

        public void Complete() => _chunks.Writer.TryComplete();

        public override async ValueTask<int> ReadAsync(
            Memory<byte> buffer,
            CancellationToken cancellationToken = default)
        {
            while (_current is null || _offset >= _current.Length)
            {
                if (!await _chunks.Reader.WaitToReadAsync(cancellationToken))
                {
                    return 0;
                }

                if (_chunks.Reader.TryRead(out _current))
                {
                    _offset = 0;
                }
            }

            var count = Math.Min(buffer.Length, _current.Length - _offset);
            _current.AsMemory(_offset, count).CopyTo(buffer);
            _offset += count;
            return count;
        }

        public override int Read(byte[] buffer, int offset, int count) =>
            ReadAsync(buffer.AsMemory(offset, count)).AsTask().GetAwaiter().GetResult();

        public override bool CanRead => true;
        public override bool CanSeek => false;
        public override bool CanWrite => false;
        public override long Length => throw new NotSupportedException();
        public override long Position { get => throw new NotSupportedException(); set => throw new NotSupportedException(); }
        public override void Flush() { }
        public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();
        public override void SetLength(long value) => throw new NotSupportedException();
        public override void Write(byte[] buffer, int offset, int count) => throw new NotSupportedException();
    }
}
