using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentHostLifecycle : ILocalAgentHostLifecycle, IAsyncDisposable
{
    private const int ProtocolVersion = 25;
    private readonly LocalAgentHostOptions _options;
    private readonly ILocalAgentHostProcessLauncher _launcher;
    private readonly SemaphoreSlim _gate = new(1, 1);
    private ILocalAgentHostProcess? _process;

    public WindowsLocalAgentHostLifecycle(LocalAgentHostOptions options)
        : this(options, new LocalAgentHostProcessLauncher())
    {
    }

    internal WindowsLocalAgentHostLifecycle(
        LocalAgentHostOptions options,
        ILocalAgentHostProcessLauncher launcher)
    {
        _options = options;
        _launcher = launcher;
    }

    public string? ActiveOwnerUserId { get; private set; }

    public async Task StartForOwnerAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default)
    {
        ValidateOwner(ownerUserId);
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_process is { HasExited: false } &&
                string.Equals(ActiveOwnerUserId, ownerUserId, StringComparison.Ordinal))
            {
                return;
            }
            await StopLockedAsync().ConfigureAwait(false);
            var process = await _launcher
                .LaunchAsync(_options, ownerUserId, cancellationToken)
                .ConfigureAwait(false);
            _process = process;
            try
            {
                using var startup = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
                startup.CancelAfter(_options.StartupTimeout);
                await VerifyHealthAsync(process, startup.Token).ConfigureAwait(false);
                ActiveOwnerUserId = ownerUserId;
            }
            catch
            {
                await StopLockedAsync().ConfigureAwait(false);
                throw;
            }
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task StopAsync(CancellationToken cancellationToken = default)
    {
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try { await StopLockedAsync().ConfigureAwait(false); }
        finally { _gate.Release(); }
    }

    private static async Task VerifyHealthAsync(
        ILocalAgentHostProcess process,
        CancellationToken cancellationToken)
    {
        var commandId = $"native-health-{Guid.NewGuid():N}";
        var request = JsonSerializer.SerializeToUtf8Bytes(new
        {
            protocol_version = ProtocolVersion,
            command_id = commandId,
            command = new { type = "health" },
        });
        await LocalAgentHostFrameCodec
            .WriteAsync(process.StandardInput, request, cancellationToken)
            .ConfigureAwait(false);
        var payload = await LocalAgentHostFrameCodec
            .ReadAsync(process.StandardOutput, cancellationToken)
            .ConfigureAwait(false);
        var response = JsonSerializer.Deserialize<HostResponse>(payload)
            ?? throw new InvalidDataException("Local Agent Host returned an empty response.");
        if (response.ProtocolVersion != ProtocolVersion || response.CommandId != commandId)
        {
            throw new InvalidDataException("Local Agent Host response identity is invalid.");
        }
        if (!response.Ok)
        {
            throw new InvalidOperationException(
                response.Error?.Message ?? "Local Agent Host health check failed.");
        }
        if (response.Result is not { } result ||
            !result.TryGetProperty("type", out var type) || type.GetString() != "health" ||
            !result.TryGetProperty("storage_ready", out var ready) || !ready.GetBoolean())
        {
            throw new InvalidDataException("Local Agent Host health payload is invalid.");
        }
    }

    private async Task StopLockedAsync()
    {
        var process = _process;
        _process = null;
        ActiveOwnerUserId = null;
        if (process is null) return;
        await process.TerminateAsync().ConfigureAwait(false);
        await process.DisposeAsync().ConfigureAwait(false);
    }

    private static void ValidateOwner(string ownerUserId)
    {
        if (string.IsNullOrWhiteSpace(ownerUserId) || ownerUserId.Length > 256 ||
            ownerUserId.Any(char.IsControl))
        {
            throw new ArgumentException(
                "Local Agent owner must be 1..=256 non-control characters.",
                nameof(ownerUserId));
        }
    }

    public async ValueTask DisposeAsync()
    {
        await StopAsync().ConfigureAwait(false);
        _gate.Dispose();
    }

    private sealed record HostResponse(
        [property: JsonPropertyName("protocol_version")] int ProtocolVersion,
        [property: JsonPropertyName("command_id")] string CommandId,
        [property: JsonPropertyName("ok")] bool Ok,
        [property: JsonPropertyName("result")] JsonElement? Result,
        [property: JsonPropertyName("error")] HostError? Error);

    private sealed record HostError(
        [property: JsonPropertyName("message")] string Message);
}
