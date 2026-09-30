using System.Text.Json;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentHostLifecycle : ILocalAgentHostClient, IAsyncDisposable
{
    private const int ProtocolVersion = 25;
    private static readonly JsonSerializerOptions SerializerOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
    };
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
            await StartLockedAsync(
                ownerUserId,
                new Dictionary<string, string>(),
                cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task RestartForOwnerAsync(
        string ownerUserId,
        IReadOnlyDictionary<string, string> credentialEnvironment,
        CancellationToken cancellationToken = default)
    {
        ValidateOwner(ownerUserId);
        ValidateCredentialEnvironment(credentialEnvironment);
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await StopLockedAsync().ConfigureAwait(false);
            await StartLockedAsync(
                ownerUserId,
                credentialEnvironment,
                cancellationToken).ConfigureAwait(false);
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

    public async Task<TResponse> SendAsync<TCommand, TResponse>(
        TCommand command,
        CancellationToken cancellationToken = default)
        where TCommand : notnull
    {
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_process is not { HasExited: false } process || ActiveOwnerUserId is null)
            {
                throw new InvalidOperationException("Local Agent Host is not running.");
            }
            var commandElement = JsonSerializer.SerializeToElement(command, SerializerOptions);
            if (commandElement.ValueKind != JsonValueKind.Object ||
                !commandElement.TryGetProperty("type", out var type) ||
                type.ValueKind != JsonValueKind.String)
            {
                throw new InvalidDataException(
                    "Local Agent Host command must be a JSON object with a type.");
            }
            if (commandElement.TryGetProperty("owner_user_id", out var owner) &&
                !string.Equals(owner.GetString(), ActiveOwnerUserId, StringComparison.Ordinal))
            {
                throw new InvalidOperationException(
                    "Local Agent Host command owner does not match the active account.");
            }
            return await RoundTripAsync<TResponse>(
                process,
                commandElement,
                cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            _gate.Release();
        }
    }

    private static async Task VerifyHealthAsync(
        ILocalAgentHostProcess process,
        CancellationToken cancellationToken)
    {
        var commandId = $"native-health-{Guid.NewGuid():N}";
        var request = JsonSerializer.SerializeToUtf8Bytes(
            new HostRequest<JsonElement>(
                ProtocolVersion,
                commandId,
                JsonSerializer.SerializeToElement(new { type = "health" }, SerializerOptions)),
            SerializerOptions);
        await LocalAgentHostFrameCodec
            .WriteAsync(process.StandardInput, request, cancellationToken)
            .ConfigureAwait(false);
        var payload = await LocalAgentHostFrameCodec
            .ReadAsync(process.StandardOutput, cancellationToken)
            .ConfigureAwait(false);
        var response = JsonSerializer.Deserialize<HostResponse>(payload, SerializerOptions)
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
            result.Deserialize<HealthResult>(SerializerOptions) is not
                { Type: "health", StorageReady: true })
        {
            throw new InvalidDataException("Local Agent Host health payload is invalid.");
        }
    }

    private async Task StartLockedAsync(
        string ownerUserId,
        IReadOnlyDictionary<string, string> credentialEnvironment,
        CancellationToken cancellationToken)
    {
        var process = await _launcher
            .LaunchAsync(_options, ownerUserId, credentialEnvironment, cancellationToken)
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

    private static async Task<TResponse> RoundTripAsync<TResponse>(
        ILocalAgentHostProcess process,
        JsonElement command,
        CancellationToken cancellationToken)
    {
        var commandId = $"native-command-{Guid.NewGuid():N}";
        var request = JsonSerializer.SerializeToUtf8Bytes(
            new HostRequest<JsonElement>(ProtocolVersion, commandId, command),
            SerializerOptions);
        await LocalAgentHostFrameCodec
            .WriteAsync(process.StandardInput, request, cancellationToken)
            .ConfigureAwait(false);
        var payload = await LocalAgentHostFrameCodec
            .ReadAsync(process.StandardOutput, cancellationToken)
            .ConfigureAwait(false);
        var response = JsonSerializer.Deserialize<HostResponse>(
            payload,
            SerializerOptions) ?? throw new InvalidDataException(
                "Local Agent Host returned an empty response.");
        if (response.ProtocolVersion != ProtocolVersion || response.CommandId != commandId)
        {
            throw new InvalidDataException("Local Agent Host response identity is invalid.");
        }
        if (!response.Ok)
        {
            throw new LocalAgentHostRequestException(
                response.Error?.Code ?? "host_error",
                response.Error?.Message ?? "Local Agent Host request failed.",
                response.Error?.Retryable ?? false);
        }
        if (response.Result is not { } result)
        {
            throw new InvalidDataException(
                "Local Agent Host response did not contain a result.");
        }
        var decoded = result.Deserialize<TResponse>(SerializerOptions);
        return decoded is null
            ? throw new InvalidDataException("Local Agent Host result could not be decoded.")
            : decoded;
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

    private static void ValidateCredentialEnvironment(
        IReadOnlyDictionary<string, string> credentialEnvironment)
    {
        if (credentialEnvironment.Count > 64)
        {
            throw new ArgumentException(
                "Local Agent Host accepts at most 64 model credentials.",
                nameof(credentialEnvironment));
        }
        foreach (var (name, value) in credentialEnvironment)
        {
            if (!name.StartsWith("CHATOS_LOCAL_AGENT_MODEL_", StringComparison.Ordinal) ||
                name.Length > 128 ||
                name.Any(character => character != '_' &&
                    !char.IsAsciiLetterUpper(character) && !char.IsAsciiDigit(character)) ||
                string.IsNullOrEmpty(value) ||
                System.Text.Encoding.UTF8.GetByteCount(value) > 64 * 1024 ||
                value.Contains('\0'))
            {
                throw new ArgumentException(
                    "Local Agent Host credential environment is invalid.",
                    nameof(credentialEnvironment));
            }
        }
    }

    public async ValueTask DisposeAsync()
    {
        await StopAsync().ConfigureAwait(false);
        _gate.Dispose();
    }

    private sealed record HostRequest<TCommand>(
        int ProtocolVersion,
        string CommandId,
        TCommand Command);

    private sealed record HostResponse(
        int ProtocolVersion,
        string CommandId,
        bool Ok,
        JsonElement? Result,
        HostError? Error);

    private sealed record HealthResult(string Type, bool StorageReady);

    private sealed record HostError(
        string Code,
        string Message,
        bool Retryable);
}

public sealed class LocalAgentHostRequestException(
    string code,
    string message,
    bool retryable) : Exception(message)
{
    public string Code { get; } = code;

    public bool Retryable { get; } = retryable;
}
