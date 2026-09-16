using System.Threading.Channels;
using ChatOS.Connector.Remote;
using ChatOS.Connector.Terminal;

namespace ChatOS.Connector.Tests;

public sealed class SshNetRemoteTerminalSessionTests
{
    [Theory]
    [InlineData(null, null)]
    [InlineData("~", null)]
    [InlineData("/srv/app", "cd -- '/srv/app'")]
    [InlineData("~/project's files", "cd -- \"$HOME\"/'project'\\''s files'")]
    public void StartupDirectoryIsShellQuoted(string? directory, string? expected)
    {
        Assert.Equal(expected, SshNetRemoteTerminalSessionFactory.StartupCommand(directory));
    }

    [Fact]
    public async Task PreservesUtf8AcrossReadsAndPublishesSequencedOutput()
    {
        var channel = new FakeChannel();
        await using var session = new SshNetRemoteTerminalSession(Identity(), channel);
        var events = new List<TerminalEvent>();
        session.EventReceived += (_, value) =>
        {
            lock (events) events.Add(value);
        };
        session.Start();

        channel.Receive([0xE4, 0xB8]);
        channel.Receive([0xAD, (byte)'!']);
        await WaitUntilAsync(() =>
        {
            lock (events) return events.Any(value => value.Kind == TerminalEventKind.Output);
        });

        TerminalEvent[] output;
        lock (events)
        {
            output = events.Where(value => value.Kind == TerminalEventKind.Output).ToArray();
        }
        Assert.Equal("中!", string.Concat(output.Select(value => value.Data)));
        Assert.Equal(new long[] { 1 }, output.Select(value => value.Sequence!.Value).ToArray());
        var snapshot = session.SnapshotState();
        Assert.Equal("中!", snapshot.Data);
        Assert.Equal(1, snapshot.Sequence);
    }

    [Fact]
    public async Task WritesRawKeystrokesResizesAndRejectsOversizedInput()
    {
        var channel = new FakeChannel();
        await using var session = new SshNetRemoteTerminalSession(Identity(), channel);
        session.Start();

        await session.WriteAsync("pwd\r");
        await session.ResizeAsync(TerminalSize.Normalize(120, 40));

        Assert.Equal("pwd\r", System.Text.Encoding.UTF8.GetString(channel.Written.ToArray()));
        Assert.Equal(TerminalSize.Normalize(120, 40), channel.Size);
        await Assert.ThrowsAsync<ArgumentException>(() =>
            session.WriteAsync(new string('x', TerminalTransportChunker.MaximumInputBytes + 1)));
    }

    private static RemoteTerminalSessionIdentity Identity() =>
        new("terminal-1", "workspace-1", "connection-1");

    private static async Task WaitUntilAsync(Func<bool> condition)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        while (!condition())
        {
            await Task.Delay(5, timeout.Token);
        }
    }

    private sealed class FakeChannel : IRemoteTerminalChannel
    {
        private readonly Channel<byte[]> _reads = Channel.CreateUnbounded<byte[]>();
        private byte[]? _pending;
        private int _pendingOffset;

        public MemoryStream Written { get; } = new();

        public TerminalSize? Size { get; private set; }

        public void Receive(byte[] value) => _reads.Writer.TryWrite(value);

        public async ValueTask<int> ReadAsync(
            Memory<byte> buffer,
            CancellationToken cancellationToken)
        {
            var pending = _pending;
            if (pending is null)
            {
                pending = await _reads.Reader.ReadAsync(cancellationToken);
                _pending = pending;
                _pendingOffset = 0;
            }
            if (pending.Length == 0)
            {
                return 0;
            }

            var count = Math.Min(buffer.Length, pending.Length - _pendingOffset);
            pending.AsMemory(_pendingOffset, count).CopyTo(buffer);
            _pendingOffset += count;
            if (_pendingOffset == pending.Length)
            {
                _pending = null;
            }
            return count;
        }

        public ValueTask WriteAsync(
            ReadOnlyMemory<byte> buffer,
            CancellationToken cancellationToken)
        {
            Written.Write(buffer.Span);
            return ValueTask.CompletedTask;
        }

        public Task FlushAsync(CancellationToken cancellationToken) => Task.CompletedTask;

        public void Resize(TerminalSize size) => Size = size;

        public ValueTask DisposeAsync()
        {
            _reads.Writer.TryWrite([]);
            return ValueTask.CompletedTask;
        }
    }
}
