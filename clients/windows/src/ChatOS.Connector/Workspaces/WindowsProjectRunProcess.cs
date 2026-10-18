using System.Diagnostics;
using System.Text;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Workspaces;

internal sealed class WindowsProjectRunProcess : IDisposable
{
    private const int MaximumLogCharacters = 200_000;
    private readonly object _gate = new();
    private readonly Process _process;
    private readonly StringBuilder _log;
    private readonly WindowsProcessJob? _windowsJob;
    private string _status = "starting";
    private bool _isRunning;
    private bool _stopRequested;
    private int? _exitCode;

    public WindowsProjectRunProcess(
        string id,
        string projectId,
        ProjectRunTarget target,
        IReadOnlyDictionary<string, string> environment)
    {
        Id = id;
        ProjectId = projectId;
        Target = target;
        StartedAt = DateTimeOffset.UtcNow;
        _log = new StringBuilder($"> {target.Command}{Environment.NewLine}");
        _windowsJob = OperatingSystem.IsWindows() ? new WindowsProcessJob() : null;
        _process = new Process
        {
            StartInfo = StartInfo(target, environment),
            EnableRaisingEvents = true,
        };
        _process.OutputDataReceived += (_, eventArgs) => Append(eventArgs.Data);
        _process.ErrorDataReceived += (_, eventArgs) => Append(eventArgs.Data);
        _process.Exited += (_, _) => Complete();
    }

    public string Id { get; }

    public string ProjectId { get; }

    public ProjectRunTarget Target { get; }

    public DateTimeOffset StartedAt { get; }

    public void Start()
    {
        try
        {
            lock (_gate)
            {
                if (!_process.Start())
                {
                    throw new InvalidOperationException("The project process did not start.");
                }
                _windowsJob?.Assign(_process);
                _status = "running";
                _isRunning = true;
            }
            _process.BeginOutputReadLine();
            _process.BeginErrorReadLine();
        }
        catch
        {
            lock (_gate) _status = "failed";
            Dispose();
            throw;
        }
    }

    public void Stop()
    {
        lock (_gate)
        {
            if (!_isRunning) return;
            _stopRequested = true;
            _status = "stopping";
        }
        try
        {
            if (_windowsJob is not null) _windowsJob.Terminate();
            else _process.Kill(entireProcessTree: true);
        }
        catch (InvalidOperationException)
        {
            Complete();
        }
    }

    public ProjectRunInstance Snapshot()
    {
        lock (_gate)
        {
            return new ProjectRunInstance(
                Id,
                Target.Label,
                Target.WorkingDirectory,
                _status,
                false,
                _isRunning,
                _log.ToString(),
                StartedAt,
                _exitCode);
        }
    }

    public void Dispose()
    {
        try
        {
            if (!_process.HasExited)
            {
                if (_windowsJob is not null) _windowsJob.Terminate();
                else _process.Kill(entireProcessTree: true);
            }
        }
        catch (InvalidOperationException)
        {
        }
        finally
        {
            _windowsJob?.Dispose();
        }
        _process.Dispose();
    }

    private void Complete()
    {
        int? exitCode = null;
        try
        {
            // A shell may exit while a detached descendant still owns the output
            // handles. End the job before draining redirected output so the run
            // cannot outlive its instance and the exit marker stays last.
            _windowsJob?.Terminate();
            _process.WaitForExit();
            exitCode = _process.ExitCode;
        }
        catch (InvalidOperationException) { }
        lock (_gate)
        {
            _exitCode = exitCode;
            _isRunning = false;
            _status = FinalStatus(_stopRequested, exitCode);
            AppendLocked($"[process exited with code {exitCode?.ToString() ?? "unknown"}]");
        }
    }

    internal static string FinalStatus(bool stopRequested, int? exitCode) =>
        stopRequested ? "stopped" : exitCode is 0 ? "exited" : "failed";

    private void Append(string? line)
    {
        if (line is null) return;
        lock (_gate) AppendLocked(line);
    }

    private void AppendLocked(string line)
    {
        _log.AppendLine(line);
        if (_log.Length > MaximumLogCharacters)
        {
            _log.Remove(0, _log.Length - MaximumLogCharacters);
        }
    }

    private static ProcessStartInfo StartInfo(
        ProjectRunTarget target,
        IReadOnlyDictionary<string, string> environment)
    {
        var start = OperatingSystem.IsWindows()
            ? new ProcessStartInfo(Environment.GetEnvironmentVariable("COMSPEC") ?? "cmd.exe")
            : new ProcessStartInfo("/bin/zsh");
        if (OperatingSystem.IsWindows())
        {
            start.ArgumentList.Add("/d");
            start.ArgumentList.Add("/s");
            start.ArgumentList.Add("/c");
        }
        else
        {
            start.ArgumentList.Add("-lc");
        }
        start.ArgumentList.Add(target.Command);
        start.WorkingDirectory = target.WorkingDirectory;
        start.UseShellExecute = false;
        start.CreateNoWindow = true;
        start.RedirectStandardOutput = true;
        start.RedirectStandardError = true;
        foreach (var pair in environment) start.Environment[pair.Key] = pair.Value;
        return start;
    }
}
