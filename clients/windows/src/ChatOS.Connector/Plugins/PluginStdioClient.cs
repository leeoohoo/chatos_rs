using System.Collections.Concurrent;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace ChatOS.Connector.Plugins;

internal interface IPluginMcpClient : IAsyncDisposable
{
    Task StartAsync(CancellationToken cancellationToken = default);

    Task<PluginMcpInitialization> InitializeAsync(CancellationToken cancellationToken = default);

    Task<JsonElement> CallToolAsync(
        string name,
        JsonElement arguments,
        TimeSpan timeout,
        CancellationToken cancellationToken = default);

    Task TerminateAsync();
}

internal interface IPluginMcpClientFactory
{
    IPluginMcpClient Create(PreparedPluginLaunch launch);
}

internal sealed class PluginMcpClientFactory(
    IHttpClientFactory httpClients,
    PluginOAuthBroker oauth) : IPluginMcpClientFactory
{
    public IPluginMcpClient Create(PreparedPluginLaunch launch) => launch.Transport switch
    {
        "stdio" => new PluginStdioClient(launch),
        "http" => new PluginHttpMcpClient(launch, httpClients, oauth),
        _ => throw new PluginRuntimeException("Plugin MCP transport is not supported."),
    };
}

public sealed class PluginStdioClient : IPluginMcpClient
{
    internal const int MaximumMessageBytes = 8 * 1024 * 1024;
    internal const int MaximumErrorBytes = 16 * 1024;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);
    private static readonly Regex SensitiveAssignment = new(
        "(?i)(authorization|password|passwd|secret|token|cookie)(\\s*[:=]\\s*)([^,; ]+)",
        RegexOptions.Compiled | RegexOptions.CultureInvariant);
    private static readonly Regex BearerValue = new(
        "(?i)(bearer\\s+)[A-Za-z0-9._~+/=-]+",
        RegexOptions.Compiled | RegexOptions.CultureInvariant);
    private readonly PreparedPluginLaunch _launch;
    private readonly IPluginProcessLauncher _launcher;
    private readonly ConcurrentDictionary<long, TaskCompletionSource<JsonElement>> _pending = new();
    private readonly SemaphoreSlim _writeGate = new(1, 1);
    private readonly object _stderrGate = new();
    private readonly MemoryStream _stderrTail = new(MaximumErrorBytes);
    private IPluginProcess? _process;
    private Task? _outputReader;
    private Task? _errorReader;
    private Task? _exitObserver;
    private CancellationTokenSource? _lifetime;
    private long _nextRequestId;
    private int _stopped;

    internal PluginStdioClient(
        PreparedPluginLaunch launch,
        IPluginProcessLauncher? launcher = null)
    {
        _launch = launch;
        _launcher = launcher ?? new WindowsPluginProcessLauncher();
    }

    public async Task StartAsync(CancellationToken cancellationToken = default)
    {
        if (_process is not null)
        {
            return;
        }

        var process = await _launcher.LaunchAsync(_launch, cancellationToken).ConfigureAwait(false);
        _process = process;
        _lifetime = new CancellationTokenSource();
        _outputReader = ReadOutputAsync(process.StandardOutput, _lifetime.Token);
        _errorReader = ReadErrorAsync(process.StandardError, _lifetime.Token);
        _exitObserver = ObserveExitAsync(process);
    }

    public async Task<PluginMcpInitialization> InitializeAsync(
        CancellationToken cancellationToken = default)
    {
        var initialized = await RequestAsync(
            "initialize",
            JsonSerializer.SerializeToElement(new
            {
                protocolVersion = "2024-11-05",
                capabilities = new { },
                clientInfo = new { name = "ChatOS Windows", version = "1" },
            }, JsonOptions),
            TimeSpan.FromSeconds(30),
            cancellationToken).ConfigureAwait(false);
        var instructions = initialized.ValueKind == JsonValueKind.Object &&
            initialized.TryGetProperty("instructions", out var instructionsValue) &&
            instructionsValue.ValueKind == JsonValueKind.String
                ? instructionsValue.GetString()?.Trim()
                : null;

        await NotifyAsync(
            "notifications/initialized",
            JsonSerializer.SerializeToElement(new { }, JsonOptions),
            cancellationToken).ConfigureAwait(false);
        var listed = await RequestAsync(
            "tools/list",
            JsonSerializer.SerializeToElement(new { }, JsonOptions),
            TimeSpan.FromSeconds(30),
            cancellationToken).ConfigureAwait(false);
        if (listed.ValueKind != JsonValueKind.Object ||
            !listed.TryGetProperty("tools", out var toolsValue) ||
            toolsValue.ValueKind != JsonValueKind.Array)
        {
            throw await StopWithAsync("Plugin MCP did not publish a valid tool list.").ConfigureAwait(false);
        }

        var tools = toolsValue.EnumerateArray().Select(tool => tool.Clone()).ToArray();
        if (tools.Length == 0)
        {
            throw await StopWithAsync("Plugin MCP did not publish any tools.").ConfigureAwait(false);
        }

        return new PluginMcpInitialization(
            string.IsNullOrWhiteSpace(instructions) ? null : instructions,
            tools);
    }

    public Task<JsonElement> CallToolAsync(
        string name,
        JsonElement arguments,
        TimeSpan timeout,
        CancellationToken cancellationToken = default) =>
        RequestAsync(
            "tools/call",
            JsonSerializer.SerializeToElement(new
            {
                name,
                arguments,
            }, JsonOptions),
            timeout,
            cancellationToken);

    public async Task TerminateAsync()
    {
        if (Interlocked.Exchange(ref _stopped, 1) != 0)
        {
            return;
        }

        _lifetime?.Cancel();
        var process = _process;
        if (process is not null)
        {
            await process.TerminateAsync().ConfigureAwait(false);
        }

        FailPending(new PluginRuntimeException("Plugin MCP call was cancelled."));
    }

    public async ValueTask DisposeAsync()
    {
        await TerminateAsync().ConfigureAwait(false);
        var readers = new[] { _outputReader, _errorReader, _exitObserver }.Where(task => task is not null).Cast<Task>();
        try
        {
            await Task.WhenAll(readers).WaitAsync(TimeSpan.FromSeconds(5)).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is OperationCanceledException or TimeoutException or PluginRuntimeException)
        {
        }

        if (_process is not null)
        {
            await _process.DisposeAsync().ConfigureAwait(false);
        }

        _lifetime?.Dispose();
        _writeGate.Dispose();
        _stderrTail.Dispose();
    }

    private async Task<JsonElement> RequestAsync(
        string method,
        JsonElement parameters,
        TimeSpan timeout,
        CancellationToken cancellationToken)
    {
        var process = _process;
        if (process is null || process.HasExited || Volatile.Read(ref _stopped) != 0)
        {
            throw new PluginRuntimeException("Plugin MCP process is unavailable.");
        }

        var id = Interlocked.Increment(ref _nextRequestId);
        var completion = new TaskCompletionSource<JsonElement>(TaskCreationOptions.RunContinuationsAsynchronously);
        if (!_pending.TryAdd(id, completion))
        {
            throw new PluginRuntimeException("Plugin MCP request id collision.");
        }

        try
        {
            await WriteAsync(new
            {
                jsonrpc = "2.0",
                id,
                method,
                @params = parameters,
            }, cancellationToken).ConfigureAwait(false);
            try
            {
                return await completion.Task.WaitAsync(timeout, cancellationToken).ConfigureAwait(false);
            }
            catch (TimeoutException exception)
            {
                await TerminateAsync().ConfigureAwait(false);
                throw new PluginRuntimeException("Plugin MCP call timed out.", exception);
            }
            catch (OperationCanceledException)
            {
                await TerminateAsync().ConfigureAwait(false);
                throw;
            }
        }
        finally
        {
            _pending.TryRemove(id, out _);
        }
    }

    private Task NotifyAsync(
        string method,
        JsonElement parameters,
        CancellationToken cancellationToken) =>
        WriteAsync(new
        {
            jsonrpc = "2.0",
            method,
            @params = parameters,
        }, cancellationToken);

    private async Task WriteAsync(object envelope, CancellationToken cancellationToken)
    {
        var process = _process ?? throw new PluginRuntimeException("Plugin MCP process is unavailable.");
        var data = JsonSerializer.SerializeToUtf8Bytes(envelope, JsonOptions);
        if (data.Length > MaximumMessageBytes)
        {
            throw new PluginRuntimeException("Plugin MCP request exceeds the size limit.");
        }

        await _writeGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await process.StandardInput.WriteAsync(data, cancellationToken).ConfigureAwait(false);
            await process.StandardInput.WriteAsync("\n"u8.ToArray(), cancellationToken).ConfigureAwait(false);
            await process.StandardInput.FlushAsync(cancellationToken).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is IOException or ObjectDisposedException)
        {
            await TerminateAsync().ConfigureAwait(false);
            throw new PluginRuntimeException("Plugin MCP request could not be written.", exception);
        }
        finally
        {
            _writeGate.Release();
        }
    }

    private async Task ReadOutputAsync(Stream output, CancellationToken cancellationToken)
    {
        var buffer = new byte[64 * 1024];
        using var line = new MemoryStream();
        try
        {
            while (true)
            {
                var read = await output.ReadAsync(buffer, cancellationToken).ConfigureAwait(false);
                if (read == 0)
                {
                    return;
                }

                for (var index = 0; index < read; index++)
                {
                    if (buffer[index] == (byte)'\n')
                    {
                        if (line.Length > 0)
                        {
                            ConsumeLine(line.GetBuffer().AsSpan(0, checked((int)line.Length)));
                            line.SetLength(0);
                        }

                        continue;
                    }

                    if (line.Length >= MaximumMessageBytes)
                    {
                        throw new PluginRuntimeException("Plugin MCP response exceeds the size limit.");
                    }

                    line.WriteByte(buffer[index]);
                }
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await StopAfterReaderFailureAsync(exception).ConfigureAwait(false);
        }
    }

    private void ConsumeLine(ReadOnlySpan<byte> line)
    {
        try
        {
            using var document = JsonDocument.Parse(line.ToArray());
            var root = document.RootElement;
            if (root.ValueKind != JsonValueKind.Object ||
                !root.TryGetProperty("id", out var idValue) ||
                !idValue.TryGetInt64(out var id) ||
                !_pending.TryRemove(id, out var completion))
            {
                return;
            }

            if (root.TryGetProperty("error", out var error) && error.ValueKind == JsonValueKind.Object)
            {
                var message = error.TryGetProperty("message", out var value) && value.ValueKind == JsonValueKind.String
                    ? value.GetString() ?? "Plugin MCP call failed."
                    : "Plugin MCP call failed.";
                completion.TrySetException(new PluginRuntimeException(message));
                return;
            }

            completion.TrySetResult(
                root.TryGetProperty("result", out var result)
                    ? result.Clone()
                    : JsonSerializer.SerializeToElement<object?>(null));
        }
        catch (JsonException exception)
        {
            throw new PluginRuntimeException("Plugin MCP returned invalid JSON.", exception);
        }
    }

    private async Task ReadErrorAsync(Stream error, CancellationToken cancellationToken)
    {
        var buffer = new byte[4 * 1024];
        try
        {
            while (true)
            {
                var read = await error.ReadAsync(buffer, cancellationToken).ConfigureAwait(false);
                if (read == 0)
                {
                    return;
                }

                lock (_stderrGate)
                {
                    if (_stderrTail.Length + read > MaximumErrorBytes)
                    {
                        var existing = _stderrTail.ToArray();
                        var keep = Math.Max(0, MaximumErrorBytes - read);
                        _stderrTail.SetLength(0);
                        if (keep > 0)
                        {
                            _stderrTail.Write(existing, Math.Max(0, existing.Length - keep), Math.Min(keep, existing.Length));
                        }
                    }

                    _stderrTail.Write(buffer, Math.Max(0, read - MaximumErrorBytes), Math.Min(read, MaximumErrorBytes));
                }
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
    }

    private async Task ObserveExitAsync(IPluginProcess process)
    {
        try
        {
            var exitCode = await process.WaitForExitAsync().ConfigureAwait(false);
            if (Volatile.Read(ref _stopped) == 0)
            {
                Interlocked.Exchange(ref _stopped, 1);
                _lifetime?.Cancel();
                var detail = RedactedErrorTail();
                FailPending(new PluginRuntimeException(
                    detail.Length == 0
                        ? $"Plugin MCP process exited with code {exitCode}."
                        : $"Plugin MCP process exited with code {exitCode}: {detail}"));
            }
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            FailPending(new PluginRuntimeException("Plugin MCP process exit could not be observed.", exception));
        }
    }

    private async Task StopAfterReaderFailureAsync(Exception exception)
    {
        if (Interlocked.Exchange(ref _stopped, 1) == 0)
        {
            _lifetime?.Cancel();
            if (_process is not null)
            {
                await _process.TerminateAsync().ConfigureAwait(false);
            }

            FailPending(exception is PluginRuntimeException
                ? exception
                : new PluginRuntimeException("Plugin MCP output reader failed.", exception));
        }
    }

    private async Task<PluginRuntimeException> StopWithAsync(string message)
    {
        await TerminateAsync().ConfigureAwait(false);
        return new PluginRuntimeException(message);
    }

    private void FailPending(Exception exception)
    {
        foreach (var pair in _pending.ToArray())
        {
            if (_pending.TryRemove(pair.Key, out var completion))
            {
                completion.TrySetException(exception);
            }
        }
    }

    private string RedactedErrorTail()
    {
        byte[] bytes;
        lock (_stderrGate)
        {
            bytes = _stderrTail.ToArray();
        }

        return RedactErrorTail(bytes);
    }

    internal static string RedactErrorTail(ReadOnlySpan<byte> bytes)
    {
        var value = Encoding.UTF8.GetString(bytes)
            .Replace('\r', ' ')
            .Replace('\n', ' ')
            .Replace('\t', ' ');
        value = new string(value.Where(character => !char.IsControl(character)).ToArray());
        value = BearerValue.Replace(value, "$1<redacted>");
        value = SensitiveAssignment.Replace(value, "$1$2<redacted>");
        value = value.Trim();
        return value.Length <= 2_000 ? value : value[^2_000..];
    }
}
