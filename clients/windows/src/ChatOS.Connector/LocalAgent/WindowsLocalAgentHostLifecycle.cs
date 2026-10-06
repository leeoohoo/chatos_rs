using System.Collections.Concurrent;
using System.Text.Json;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentHostLifecycle : ILocalAgentHostClient, IAsyncDisposable
{
    private const int ProtocolVersion = 40;
    private static readonly JsonSerializerOptions SerializerOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
    };
    private readonly LocalAgentHostOptions _options;
    private readonly ILocalAgentHostProcessLauncher _launcher;
    private readonly SemaphoreSlim _transition = new(1, 1);
    private readonly object _stateGate = new();
    private RunningHost? _running;
    private string? _activeOwnerUserId;
    private ulong _generation;
    private bool _disposed;
    private int _disposeStarted;

    public WindowsLocalAgentHostLifecycle(LocalAgentHostOptions options)
        : this(options, new LocalAgentHostProcessLauncher())
    {
    }

    internal WindowsLocalAgentHostLifecycle(
        LocalAgentHostOptions options,
        ILocalAgentHostProcessLauncher launcher)
    {
        if (options.RequestTimeout <= TimeSpan.Zero ||
            options.RequestTimeout > TimeSpan.FromMinutes(5))
        {
            throw new ArgumentOutOfRangeException(
                nameof(options), "Local Agent Host request timeout must be between 1 ms and 5 minutes.");
        }
        _options = options;
        _launcher = launcher;
    }

    public event EventHandler? UnexpectedExit;

    public string? ActiveOwnerUserId
    {
        get { lock (_stateGate) return _activeOwnerUserId; }
    }

    public async Task StartForOwnerAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default)
    {
        ValidateOwner(ownerUserId);
        await _transition.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            ThrowIfDisposed();
            lock (_stateGate)
            {
                if (_running is { IsAlive: true } &&
                    string.Equals(_activeOwnerUserId, ownerUserId, StringComparison.Ordinal))
                {
                    return;
                }
            }
            await ReplaceAsync(ownerUserId, new Dictionary<string, string>(), cancellationToken)
                .ConfigureAwait(false);
        }
        finally
        {
            _transition.Release();
        }
    }

    public async Task RestartForOwnerAsync(
        string ownerUserId,
        IReadOnlyDictionary<string, string> credentialEnvironment,
        CancellationToken cancellationToken = default)
    {
        ValidateOwner(ownerUserId);
        ValidateCredentialEnvironment(credentialEnvironment);
        await _transition.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            ThrowIfDisposed();
            await ReplaceAsync(ownerUserId, credentialEnvironment, cancellationToken)
                .ConfigureAwait(false);
        }
        finally
        {
            _transition.Release();
        }
    }

    public async Task StopAsync(CancellationToken cancellationToken = default)
    {
        await _transition.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var previous = DetachCurrent();
            if (previous is not null) await previous.DisposeAsync().ConfigureAwait(false);
        }
        finally
        {
            _transition.Release();
        }
    }

    public async Task<TResponse> SendAsync<TCommand, TResponse>(
        TCommand command,
        CancellationToken cancellationToken = default)
        where TCommand : notnull
    {
        RunningHost running;
        string activeOwner;
        lock (_stateGate)
        {
            ThrowIfDisposed();
            running = _running is { IsAlive: true } value
                ? value
                : throw new InvalidOperationException("Local Agent Host is not running.");
            activeOwner = _activeOwnerUserId
                ?? throw new InvalidOperationException("Local Agent Host is not running.");
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
            !string.Equals(owner.GetString(), activeOwner, StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                "Local Agent Host command owner does not match the active account.");
        }

        var commandId = $"native-command-{Guid.NewGuid():N}";
        byte[] payload;
        try
        {
            payload = await running.RoundTripAsync(
                Envelope(commandId, commandElement), commandId, _options.RequestTimeout,
                cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch
        {
            InvalidateAfterFailure(running);
            throw;
        }
        return DecodeResponse<TResponse>(payload, commandId);
    }

    private async Task ReplaceAsync(
        string ownerUserId,
        IReadOnlyDictionary<string, string> credentialEnvironment,
        CancellationToken cancellationToken)
    {
        var previous = DetachCurrent();
        if (previous is not null) await previous.DisposeAsync().ConfigureAwait(false);

        var process = await _launcher
            .LaunchAsync(_options, ownerUserId, credentialEnvironment, cancellationToken)
            .ConfigureAwait(false);
        var generation = NextGeneration();
        var running = new RunningHost(process, generation, OnTransportFailed);
        lock (_stateGate) _running = running;
        try
        {
            var commandId = $"native-health-{Guid.NewGuid():N}";
            var command = JsonSerializer.SerializeToElement(
                new { type = "health" }, SerializerOptions);
            var payload = await running.RoundTripAsync(
                Envelope(commandId, command), commandId, _options.StartupTimeout,
                cancellationToken).ConfigureAwait(false);
            var health = DecodeResponse<HealthResult>(payload, commandId);
            if (health is not { Type: "health", StorageReady: true })
                throw new InvalidDataException("Local Agent Host health payload is invalid.");
            lock (_stateGate)
            {
                if (!ReferenceEquals(_running, running))
                    throw new InvalidOperationException("Local Agent Host exited during startup.");
                _activeOwnerUserId = ownerUserId;
            }
        }
        catch
        {
            lock (_stateGate)
            {
                if (ReferenceEquals(_running, running))
                {
                    _running = null;
                    _activeOwnerUserId = null;
                    _generation++;
                }
            }
            await running.DisposeAsync().ConfigureAwait(false);
            throw;
        }
    }

    private static byte[] Envelope(string commandId, JsonElement command) =>
        JsonSerializer.SerializeToUtf8Bytes(
            new HostRequest<JsonElement>(ProtocolVersion, commandId, command), SerializerOptions);

    private static TResponse DecodeResponse<TResponse>(byte[] payload, string commandId)
    {
        var response = JsonSerializer.Deserialize<HostResponse>(payload, SerializerOptions)
            ?? throw new InvalidDataException("Local Agent Host returned an empty response.");
        if (response.ProtocolVersion != ProtocolVersion || response.CommandId != commandId)
            throw new InvalidDataException("Local Agent Host response identity is invalid.");
        if (!response.Ok)
        {
            throw new LocalAgentHostRequestException(
                response.Error?.Code ?? "host_error",
                response.Error?.Message ?? "Local Agent Host request failed.",
                response.Error?.Retryable ?? false);
        }
        if (response.Result is not { } result)
            throw new InvalidDataException("Local Agent Host response did not contain a result.");
        return result.Deserialize<TResponse>(SerializerOptions)
            ?? throw new InvalidDataException("Local Agent Host result could not be decoded.");
    }

    private RunningHost? DetachCurrent()
    {
        lock (_stateGate)
        {
            var previous = _running;
            _running = null;
            _activeOwnerUserId = null;
            _generation++;
            return previous;
        }
    }

    private ulong NextGeneration()
    {
        lock (_stateGate) return ++_generation;
    }

    private void OnTransportFailed(RunningHost running, Exception error) =>
        InvalidateAfterFailure(running);

    private void InvalidateAfterFailure(RunningHost failed)
    {
        var notify = false;
        lock (_stateGate)
        {
            if (!ReferenceEquals(_running, failed)) return;
            notify = _activeOwnerUserId is not null;
            _running = null;
            _activeOwnerUserId = null;
            _generation++;
        }
        _ = Task.Run(async () => await failed.DisposeAsync().ConfigureAwait(false));
        if (notify) UnexpectedExit?.Invoke(this, EventArgs.Empty);
    }

    private void ThrowIfDisposed()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(WindowsLocalAgentHostLifecycle));
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
            throw new ArgumentException("Local Agent Host accepts at most 64 model credentials.",
                nameof(credentialEnvironment));
        foreach (var (name, value) in credentialEnvironment)
        {
            if ((name != "CHATOS_MEMORY_ACCESS_TOKEN" &&
                    !name.StartsWith("CHATOS_LOCAL_AGENT_MODEL_", StringComparison.Ordinal)) ||
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
        if (Interlocked.Exchange(ref _disposeStarted, 1) != 0) return;
        await _transition.WaitAsync().ConfigureAwait(false);
        try
        {
            _disposed = true;
            var previous = DetachCurrent();
            if (previous is not null) await previous.DisposeAsync().ConfigureAwait(false);
        }
        finally
        {
            _transition.Release();
        }
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
    private sealed record HostError(string Code, string Message, bool Retryable);

    private sealed class RunningHost : IAsyncDisposable
    {
        private readonly ILocalAgentHostProcess _process;
        private readonly Action<RunningHost, Exception> _failure;
        private readonly ConcurrentDictionary<string, TaskCompletionSource<byte[]>> _pending = [];
        private readonly SemaphoreSlim _writeGate = new(1, 1);
        private readonly CancellationTokenSource _lifetime = new();
        private readonly Task _reader;
        private int _failed;
        private int _disposed;

        internal RunningHost(
            ILocalAgentHostProcess process,
            ulong generation,
            Action<RunningHost, Exception> failure)
        {
            _process = process;
            Generation = generation;
            _failure = failure;
            _process.Exited += ProcessExited;
            _reader = Task.Run(ReadResponsesAsync);
        }

        internal ulong Generation { get; }
        internal bool IsAlive => Volatile.Read(ref _failed) == 0 && !_process.HasExited;

        internal async Task<byte[]> RoundTripAsync(
            byte[] payload,
            string commandId,
            TimeSpan timeout,
            CancellationToken cancellationToken)
        {
            if (!IsAlive) throw new InvalidOperationException("Local Agent Host is not running.");
            var response = new TaskCompletionSource<byte[]>(
                TaskCreationOptions.RunContinuationsAsynchronously);
            if (!_pending.TryAdd(commandId, response))
                throw new InvalidOperationException("Duplicate Local Agent Host command identifier.");
            using var deadline = CancellationTokenSource.CreateLinkedTokenSource(_lifetime.Token);
            deadline.CancelAfter(timeout);
            try
            {
                await _writeGate.WaitAsync(cancellationToken).ConfigureAwait(false);
                try
                {
                    cancellationToken.ThrowIfCancellationRequested();
                    await LocalAgentHostFrameCodec.WriteAsync(
                        _process.StandardInput, payload, deadline.Token).ConfigureAwait(false);
                }
                finally
                {
                    _writeGate.Release();
                }
                using var wait = CancellationTokenSource.CreateLinkedTokenSource(
                    cancellationToken, deadline.Token);
                return await response.Task.WaitAsync(wait.Token).ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (
                !cancellationToken.IsCancellationRequested && !_lifetime.IsCancellationRequested)
            {
                var error = new TimeoutException(
                    "Local Agent Host did not respond before the request deadline.");
                Fail(error);
                throw error;
            }
            catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
            {
                throw;
            }
            catch (Exception error)
            {
                Fail(error);
                throw;
            }
            finally
            {
                _pending.TryRemove(commandId, out _);
            }
        }

        private async Task ReadResponsesAsync()
        {
            try
            {
                while (!_lifetime.IsCancellationRequested)
                {
                    var payload = await LocalAgentHostFrameCodec.ReadAsync(
                        _process.StandardOutput, _lifetime.Token).ConfigureAwait(false);
                    using var document = JsonDocument.Parse(payload);
                    if (!document.RootElement.TryGetProperty("command_id", out var value) ||
                        value.ValueKind != JsonValueKind.String || value.GetString() is not { } commandId)
                    {
                        throw new InvalidDataException(
                            "Local Agent Host response has no command identifier.");
                    }
                    if (_pending.TryRemove(commandId, out var pending))
                        pending.TrySetResult(payload);
                }
            }
            catch (OperationCanceledException) when (_lifetime.IsCancellationRequested)
            {
            }
            catch (Exception error)
            {
                Fail(error);
            }
        }

        private void ProcessExited(object? sender, EventArgs args) =>
            Fail(new EndOfStreamException("Local Agent Host exited unexpectedly."));

        private void Fail(Exception error)
        {
            if (Interlocked.Exchange(ref _failed, 1) != 0) return;
            _lifetime.Cancel();
            foreach (var pending in _pending.Values) pending.TrySetException(error);
            _pending.Clear();
            _failure(this, error);
        }

        public async ValueTask DisposeAsync()
        {
            if (Interlocked.Exchange(ref _disposed, 1) != 0) return;
            _process.Exited -= ProcessExited;
            Interlocked.Exchange(ref _failed, 1);
            _lifetime.Cancel();
            var stopped = new InvalidOperationException("Local Agent Host is not running.");
            foreach (var pending in _pending.Values) pending.TrySetException(stopped);
            _pending.Clear();
            await _process.DisposeAsync().ConfigureAwait(false);
            try { await _reader.WaitAsync(TimeSpan.FromSeconds(2)).ConfigureAwait(false); }
            catch (TimeoutException) { }
            catch (OperationCanceledException) { }
            _lifetime.Dispose();
            _writeGate.Dispose();
        }
    }
}

public sealed class LocalAgentHostRequestException(
    string code,
    string message,
    bool retryable) : Exception(message)
{
    public string Code { get; } = code;
    public bool Retryable { get; } = retryable;
}
