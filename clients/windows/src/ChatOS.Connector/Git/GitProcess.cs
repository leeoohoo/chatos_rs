using System.Diagnostics;
using System.Text;
using System.Text.RegularExpressions;

namespace ChatOS.Connector.Git;

internal sealed record GitProcessOutput(
    string StandardOutput,
    string StandardError,
    int ExitCode,
    bool StandardOutputTruncated,
    bool StandardErrorTruncated);

internal interface IGitProcess
{
    Task<GitProcessOutput> RunAsync(
        IReadOnlyList<string> arguments,
        string workingDirectory,
        IReadOnlySet<int>? allowedExitCodes = null,
        CancellationToken cancellationToken = default);
}

internal sealed partial class GitProcess : IGitProcess
{
    internal const int MaximumCapturedBytes = 8 * 1024 * 1024;
    internal static readonly TimeSpan DefaultTimeout = TimeSpan.FromMinutes(2);
    private readonly TimeSpan _timeout;
    private readonly string _executable;

    public GitProcess()
        : this(OperatingSystem.IsWindows() ? "git.exe" : "git", DefaultTimeout)
    {
    }

    internal GitProcess(string executable, TimeSpan timeout)
    {
        _executable = executable;
        _timeout = timeout;
    }

    public async Task<GitProcessOutput> RunAsync(
        IReadOnlyList<string> arguments,
        string workingDirectory,
        IReadOnlySet<int>? allowedExitCodes = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(workingDirectory);
        if (!Directory.Exists(workingDirectory))
        {
            throw new ProjectGitOperationException(
                ProjectGitErrorCode.WorkspaceUnavailable,
                "项目目录不存在或当前无法访问。");
        }

        var startInfo = new ProcessStartInfo
        {
            FileName = _executable,
            WorkingDirectory = workingDirectory,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            StandardOutputEncoding = Encoding.UTF8,
            StandardErrorEncoding = Encoding.UTF8,
        };
        foreach (var argument in arguments)
        {
            startInfo.ArgumentList.Add(argument);
        }

        startInfo.Environment["GIT_TERMINAL_PROMPT"] = "0";
        startInfo.Environment["GCM_INTERACTIVE"] = "Never";
        startInfo.Environment["LC_ALL"] = "C";
        startInfo.Environment["LANG"] = "C";

        using var process = new Process { StartInfo = startInfo };
        try
        {
            if (!process.Start())
            {
                throw new ProjectGitOperationException(
                    ProjectGitErrorCode.GitUnavailable,
                    "无法启动 Git。请先安装 Git for Windows，并确认 git.exe 已加入 PATH。");
            }
        }
        catch (Exception exception) when (exception is not ProjectGitOperationException)
        {
            throw new ProjectGitOperationException(
                ProjectGitErrorCode.GitUnavailable,
                "无法启动 Git。请先安装 Git for Windows，并确认 git.exe 已加入 PATH。",
                exception);
        }

        process.StandardInput.Close();
        var stdout = new BoundedTextCapture(MaximumCapturedBytes);
        var stderr = new BoundedTextCapture(MaximumCapturedBytes);
        var stdoutTask = stdout.DrainAsync(process.StandardOutput);
        var stderrTask = stderr.DrainAsync(process.StandardError);

        using var timeout = new CancellationTokenSource(_timeout);
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(
            cancellationToken,
            timeout.Token);
        try
        {
            await process.WaitForExitAsync(linked.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
            TryKill(process);
            await process.WaitForExitAsync(CancellationToken.None).ConfigureAwait(false);
            await Task.WhenAll(stdoutTask, stderrTask).ConfigureAwait(false);
            if (cancellationToken.IsCancellationRequested)
            {
                cancellationToken.ThrowIfCancellationRequested();
            }

            throw new ProjectGitOperationException(
                ProjectGitErrorCode.CommandTimedOut,
                "Git 操作等待超时，相关进程已经停止。");
        }

        await Task.WhenAll(stdoutTask, stderrTask).ConfigureAwait(false);
        var output = new GitProcessOutput(
            stdout.Text,
            stderr.Text,
            process.ExitCode,
            stdout.Truncated,
            stderr.Truncated);
        var accepted = allowedExitCodes ?? GitExitCodes.Success;
        if (!accepted.Contains(output.ExitCode))
        {
            throw ProjectGitOperationException.CommandFailed(
                arguments,
                output.StandardError,
                output.StandardOutputTruncated || output.StandardErrorTruncated);
        }

        return output;
    }

    private static void TryKill(Process process)
    {
        try
        {
            if (!process.HasExited)
            {
                process.Kill(entireProcessTree: true);
            }
        }
        catch (InvalidOperationException)
        {
        }
    }

    private sealed class BoundedTextCapture(int maximumBytes)
    {
        private readonly StringBuilder _value = new();
        private int _capturedBytes;

        public bool Truncated { get; private set; }

        public string Text => _value.ToString();

        public async Task DrainAsync(StreamReader reader)
        {
            var buffer = new char[16 * 1024];
            while (true)
            {
                var count = await reader.ReadAsync(buffer).ConfigureAwait(false);
                if (count == 0)
                {
                    return;
                }

                if (_capturedBytes >= maximumBytes)
                {
                    Truncated = true;
                    continue;
                }

                var text = new string(buffer, 0, count);
                var bytes = Encoding.UTF8.GetByteCount(text);
                if (_capturedBytes + bytes <= maximumBytes)
                {
                    _value.Append(text);
                    _capturedBytes += bytes;
                    continue;
                }

                Truncated = true;
                var remaining = maximumBytes - _capturedBytes;
                var acceptedCharacters = 0;
                var acceptedBytes = 0;
                foreach (var rune in text.EnumerateRunes())
                {
                    var runeBytes = rune.Utf8SequenceLength;
                    if (acceptedBytes + runeBytes > remaining)
                    {
                        break;
                    }

                    acceptedCharacters += rune.Utf16SequenceLength;
                    acceptedBytes += runeBytes;
                }

                if (acceptedCharacters > 0)
                {
                    _value.Append(text.AsSpan(0, acceptedCharacters));
                    _capturedBytes += acceptedBytes;
                }
            }
        }
    }

    [GeneratedRegex(@"(?i)(https?://)[^\s/@:]+:[^\s/@]+@")]
    internal static partial Regex UrlCredentialRegex();
}

internal static class GitExitCodes
{
    public static IReadOnlySet<int> Success { get; } = new HashSet<int> { 0 };

    public static IReadOnlySet<int> SuccessOrOne { get; } = new HashSet<int> { 0, 1 };

    public static IReadOnlySet<int> SuccessOrNotFound { get; } = new HashSet<int> { 0, 1, 2, 128 };
}

internal enum ProjectGitErrorCode
{
    GitUnavailable,
    WorkspaceUnavailable,
    NotRepository,
    RepositoryOutsideWorkspace,
    InvalidBranchName,
    EmptyCommitMessage,
    NoRemote,
    NoCurrentBranch,
    InvalidRemote,
    InvalidPath,
    CommandTimedOut,
    CommandFailed,
}

internal sealed class ProjectGitOperationException : Exception
{
    public ProjectGitOperationException(
        ProjectGitErrorCode code,
        string message,
        Exception? innerException = null)
        : base(message, innerException)
    {
        Code = code;
    }

    public ProjectGitErrorCode Code { get; }

    public static ProjectGitOperationException CommandFailed(
        IReadOnlyList<string> arguments,
        string message,
        bool truncated)
    {
        var detail = GitProcess.UrlCredentialRegex().Replace(message.Trim(), "$1***:***@");
        if (string.IsNullOrWhiteSpace(detail))
        {
            detail = "Git 命令执行失败。";
        }

        var localized = detail switch
        {
            var value when value.Contains(
                "Your local changes to the following files would be overwritten",
                StringComparison.OrdinalIgnoreCase) =>
                "当前修改会被分支切换覆盖，请先提交或暂存这些修改。",
            var value when value.Contains("CONFLICT", StringComparison.OrdinalIgnoreCase) ||
                value.Contains("Automatic merge failed", StringComparison.OrdinalIgnoreCase) =>
                "分支已进入冲突状态。请先处理冲突文件，再完成提交。",
            var value when value.Contains("no upstream branch", StringComparison.OrdinalIgnoreCase) =>
                "当前分支还没有关联远程分支，请先发布分支。",
            var value when value.Contains("nothing to commit", StringComparison.OrdinalIgnoreCase) =>
                "没有可提交的暂存修改。",
            var value when value.Contains("Author identity unknown", StringComparison.OrdinalIgnoreCase) ||
                value.Contains("Please tell me who you are", StringComparison.OrdinalIgnoreCase) =>
                "Git 还没有配置提交身份，请先设置 user.name 和 user.email。",
            _ => detail,
        };
        if (truncated)
        {
            localized += "\n输出过长，已截断。";
        }

        return new ProjectGitOperationException(
            ProjectGitErrorCode.CommandFailed,
            localized + $"\n操作：git {SafeOperation(arguments)}");
    }

    private static string SafeOperation(IReadOnlyList<string> arguments)
    {
        if (arguments.Count == 0)
        {
            return "command";
        }

        var safe = arguments.Take(2).Select(static value =>
            value.Contains("http", StringComparison.OrdinalIgnoreCase) ? "<redacted>" : value);
        return string.Join(' ', safe);
    }
}
