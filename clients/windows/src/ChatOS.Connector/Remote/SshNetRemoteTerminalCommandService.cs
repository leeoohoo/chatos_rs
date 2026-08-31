using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Remote;

public sealed class SshNetRemoteTerminalCommandService : IRemoteTerminalCommandService
{
    private const int MaximumOutputCharacters = 200_000;
    private readonly IRemoteConnectionRuntime _runtime;
    private readonly IRemoteSshSessionFactory _sessions;

    public SshNetRemoteTerminalCommandService(IRemoteConnectionRuntime runtime, IRemoteSshSessionFactory sessions)
    { _runtime = runtime; _sessions = sessions; }

    public async Task<RemoteTerminalCommandResult> ExecuteAsync(
        string connectionId,
        string command,
        string workingDirectory,
        string? verificationCode = null,
        CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(command)) throw new ArgumentException("远端命令不能为空。", nameof(command));
        var draft = await _runtime.ResolveDraftAsync(connectionId, cancellationToken).ConfigureAwait(false);
        using var session = await _sessions.ConnectAsync(draft, verificationCode, cancellationToken).ConfigureAwait(false);
        var marker = $"__CHATOS_REMOTE_{Guid.NewGuid():N}__";
        var requestedDirectory = string.IsNullOrWhiteSpace(workingDirectory)
            ? draft.DefaultRemotePath ?? "~"
            : workingDirectory.Trim();
        var changeDirectory = requestedDirectory == "~"
            ? "cd -- \"$HOME\""
            : $"cd -- {Quote(requestedDirectory)}";
        var script = $$"""
            chatos_tmp=$(mktemp -d "${TMPDIR:-/tmp}/chatos-remote.XXXXXX") || exit 70
            (
              {{changeDirectory}} || exit 72
              ulimit -f 400 2>/dev/null || true
              {
            {{command}}
              }
              printf '%s' "$?" > "$chatos_tmp/status"
              pwd > "$chatos_tmp/cwd"
            ) > "$chatos_tmp/out" 2> "$chatos_tmp/err"
            chatos_status=$(cat "$chatos_tmp/status" 2>/dev/null || printf '153')
            printf '{{marker}}OUT\n'
            head -c {{MaximumOutputCharacters}} "$chatos_tmp/out" 2>/dev/null
            printf '\n{{marker}}ERR\n'
            head -c {{MaximumOutputCharacters}} "$chatos_tmp/err" 2>/dev/null
            printf '\n{{marker}}CWD\n'
            cat "$chatos_tmp/cwd" 2>/dev/null || printf '%s' {{Quote(requestedDirectory)}}
            printf '\n{{marker}}STATUS\n%s\n' "$chatos_status"
            rm -rf -- "$chatos_tmp"
            """;
        using var sshCommand = session.TargetClient.CreateCommand(script);
        sshCommand.CommandTimeout = TimeSpan.FromMinutes(15);
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(TimeSpan.FromMinutes(15));
        await sshCommand.ExecuteAsync(timeout.Token).ConfigureAwait(false);
        return Parse(sshCommand.Result, sshCommand.Error, marker, requestedDirectory);
    }

    internal static RemoteTerminalCommandResult Parse(string response, string transportError, string marker, string fallbackDirectory)
    {
        var output = Section(response, marker + "OUT\n", marker + "ERR\n");
        var error = Section(response, marker + "ERR\n", marker + "CWD\n");
        var cwd = Section(response, marker + "CWD\n", marker + "STATUS\n").Trim();
        var statusText = After(response, marker + "STATUS\n").Split('\n', StringSplitOptions.RemoveEmptyEntries).FirstOrDefault();
        var status = int.TryParse(statusText, out var value) ? value : -1;
        if (!string.IsNullOrWhiteSpace(transportError)) error = string.IsNullOrWhiteSpace(error) ? transportError.Trim() : error + "\n" + transportError.Trim();
        return new RemoteTerminalCommandResult(TrimLimit(output), TrimLimit(error), status, string.IsNullOrWhiteSpace(cwd) ? fallbackDirectory : cwd);
    }

    private static string Section(string value, string start, string end)
    {
        var startIndex = value.IndexOf(start, StringComparison.Ordinal);
        if (startIndex < 0) return string.Empty;
        startIndex += start.Length;
        var endIndex = value.IndexOf(end, startIndex, StringComparison.Ordinal);
        return (endIndex < 0 ? value[startIndex..] : value[startIndex..endIndex]).Trim('\r', '\n');
    }

    private static string After(string value, string marker)
    {
        var index = value.IndexOf(marker, StringComparison.Ordinal);
        return index < 0 ? string.Empty : value[(index + marker.Length)..];
    }

    private static string TrimLimit(string value) => value.Length <= MaximumOutputCharacters ? value : value[..MaximumOutputCharacters] + "\n…输出已截断…";
    private static string Quote(string value) => "'" + value.Replace("'", "'\\''", StringComparison.Ordinal) + "'";
}
