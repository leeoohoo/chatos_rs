namespace ChatOS.Core.Domain;

public sealed record RemoteTerminalCommandResult(
    string Output,
    string Error,
    int ExitCode,
    string WorkingDirectory);
