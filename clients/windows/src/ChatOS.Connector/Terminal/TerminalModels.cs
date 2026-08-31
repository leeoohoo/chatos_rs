using ChatOS.NetworkGuard.Contracts;

namespace ChatOS.Connector.Terminal;

public readonly record struct TerminalSize(ushort Columns, ushort Rows)
{
    public static TerminalSize Normalize(int columns, int rows) => new(
        checked((ushort)Math.Clamp(columns, 1, 1_000)),
        checked((ushort)Math.Clamp(rows, 1, 1_000)));
}

public sealed record TerminalSessionIdentity(
    string SessionId,
    string WorkspaceId,
    string WorkspaceRoot,
    string WorkingDirectory,
    ControlledNetworkPolicyEnvelope? NetworkPolicy = null);

public enum TerminalEventKind
{
    Output,
    Snapshot,
    Exit,
    State,
    Error,
}

public sealed record TerminalEvent(
    TerminalEventKind Kind,
    string SessionId,
    string? Data = null,
    int? ExitCode = null,
    bool? Busy = null);

public interface ITerminalSession : IAsyncDisposable
{
    TerminalSessionIdentity Identity { get; }

    bool HasExited { get; }

    bool IsBusy { get; }

    event EventHandler<TerminalEvent>? EventReceived;

    Task WriteAsync(string data, CancellationToken cancellationToken = default);

    Task ResizeAsync(TerminalSize size, CancellationToken cancellationToken = default);

    string Snapshot(int maximumLines = 500);

    Task StopAsync(CancellationToken cancellationToken = default);
}

public interface ITerminalSessionFactory
{
    Task<ITerminalSession> CreateAsync(
        TerminalSessionIdentity identity,
        TerminalSize size,
        CancellationToken cancellationToken = default);
}

public sealed record TerminalCommandRequest(
    string Command,
    IReadOnlyList<string> Arguments,
    string WorkingDirectory,
    string WorkspaceRoot,
    string WorkspaceId,
    int TimeoutMilliseconds,
    ControlledNetworkPolicyEnvelope? NetworkPolicy = null);

public sealed record TerminalCommandResult(
    string Command,
    IReadOnlyList<string> Arguments,
    string WorkingDirectory,
    string WorkspaceId,
    bool Success,
    int? ExitCode,
    bool TimedOut,
    int TimeoutMilliseconds,
    string StandardOutput,
    string StandardError,
    long StandardOutputBytes,
    long StandardErrorBytes,
    bool StandardOutputTruncated,
    bool StandardErrorTruncated,
    string? Error = null,
    string? SandboxProfile = null,
    string? SandboxNetwork = null);

public interface ITerminalCommandExecutor
{
    Task<TerminalCommandResult> ExecuteAsync(
        TerminalCommandRequest request,
        CancellationToken cancellationToken = default);
}

public sealed record TerminalCommandHistoryEntry(
    string Id,
    string RequestId,
    string WorkspaceId,
    string Source,
    string Command,
    string WorkingDirectory,
    bool Success,
    int? ExitCode,
    bool TimedOut,
    int TimeoutMilliseconds,
    string StandardOutputPreview,
    string StandardErrorPreview,
    long StandardOutputBytes,
    long StandardErrorBytes,
    bool StandardOutputTruncated,
    bool StandardErrorTruncated,
    string ApprovalDecision,
    string ApprovalReason,
    string? Error,
    DateTimeOffset CreatedAt);

public interface ITerminalCommandHistoryStore
{
    Task AppendAsync(
        TerminalCommandHistoryEntry entry,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<TerminalCommandHistoryEntry>> ReadAsync(
        int limit = 1_000,
        CancellationToken cancellationToken = default);
}
