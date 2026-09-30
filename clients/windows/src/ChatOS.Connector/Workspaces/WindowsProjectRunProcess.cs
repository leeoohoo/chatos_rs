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
    private string _status = "starting";
    private bool _isRunning;
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
            _status = "stopping";
        }
        try
        {
            _process.Kill(entireProcessTree: true);
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
            if (!_process.HasExited) _process.Kill(entireProcessTree: true);
        }
        catch (InvalidOperationException)
        {
        }
        _process.Dispose();
    }

    private void Complete()
    {
        int? exitCode = null;
        try { exitCode = _process.ExitCode; }
        catch (InvalidOperationException) { }
        lock (_gate)
        {
            _exitCode = exitCode;
            _isRunning = false;
            _status = exitCode is 0 ? "exited" : "failed";
            AppendLocked($"[process exited with code {exitCode?.ToString() ?? "unknown"}]");
        }
    }

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
