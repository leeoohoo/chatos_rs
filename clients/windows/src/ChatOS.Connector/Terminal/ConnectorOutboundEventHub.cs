using System.Text.Json;
using System.Threading.Channels;

namespace ChatOS.Connector.Terminal;

public sealed class ConnectorOutboundEventHub
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);
    private readonly Channel<string> _events = Channel.CreateBounded<string>(new BoundedChannelOptions(256)
    {
        FullMode = BoundedChannelFullMode.DropOldest,
        SingleReader = true,
        SingleWriter = false,
    });

    public void Publish(TerminalEvent value)
    {
        var type = value.Kind switch
        {
            TerminalEventKind.Output => "terminal_output",
            TerminalEventKind.Snapshot => "terminal_snapshot",
            TerminalEventKind.Exit => "terminal_exit",
            TerminalEventKind.State => "terminal_state",
            TerminalEventKind.Error => "terminal_error",
            _ => throw new ArgumentOutOfRangeException(nameof(value)),
        };
        object body = value.Kind switch
        {
            TerminalEventKind.Output => new
            {
                data = value.Data ?? string.Empty,
                sequence = value.Sequence,
                protocol_version = 2,
            },
            TerminalEventKind.Snapshot => new
            {
                data = value.Data ?? string.Empty,
                base_sequence = value.BaseSequence ?? 0,
                sequence = value.Sequence ?? 0,
                truncated = value.Truncated ?? false,
                protocol_version = 2,
            },
            TerminalEventKind.Exit => new { code = value.ExitCode },
            TerminalEventKind.State => new
            {
                state = value.State ?? "ready",
                busy = value.Busy ?? false,
                protocol_version = 2,
            },
            TerminalEventKind.Error => new
            {
                error = value.Data ?? "Terminal session failed.",
                code = value.ErrorCode,
                prompt = value.Prompt,
                recoverable = value.Recoverable ?? true,
            },
            _ => new { },
        };
        _events.Writer.TryWrite(JsonSerializer.Serialize(new
        {
            type,
            terminal_session_id = value.SessionId,
            body,
        }, JsonOptions));
    }

    public ValueTask<string> ReadAsync(CancellationToken cancellationToken) =>
        _events.Reader.ReadAsync(cancellationToken);

    public void Drain()
    {
        while (_events.Reader.TryRead(out _))
        {
        }
    }
}
