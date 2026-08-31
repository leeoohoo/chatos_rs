using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IRemoteTerminalCommandService
{
    Task<RemoteTerminalCommandResult> ExecuteAsync(
        string connectionId,
        string command,
        string workingDirectory,
        string? verificationCode = null,
        CancellationToken cancellationToken = default);
}
