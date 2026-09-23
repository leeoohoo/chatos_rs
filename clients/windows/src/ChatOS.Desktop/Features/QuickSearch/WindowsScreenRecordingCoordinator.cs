using System.Diagnostics;
using ChatOS.Core.Domain;

namespace ChatOS.Desktop.Features.QuickSearch;

public sealed class WindowsScreenRecordingCoordinator : IDisposable
{
    private readonly SemaphoreSlim _gate = new(1, 1);
    private CancellationTokenSource? _monitorCancellation;
    private Task? _monitorTask;

    public ScreenRecordingState State { get; private set; } = ScreenRecordingState.Idle;

    public string? LastArchivedPath { get; private set; }

    public string? LastError { get; private set; }

    public event EventHandler? StateChanged;

    public async Task StartOrShowControlsAsync(CancellationToken cancellationToken = default)
    {
        await _gate.WaitAsync(cancellationToken);
        try
        {
            if (_monitorTask is { IsCompleted: false })
            {
                LaunchNativeRecorder();
                return;
            }

            _monitorCancellation?.Dispose();
            _monitorCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            LastArchivedPath = null;
            LastError = null;
            SetState(ScreenRecordingState.SelectingTarget);
            var startedAt = DateTimeOffset.Now;
            LaunchNativeRecorder();
            SetState(ScreenRecordingState.RecordingOrAwaitingSave);
            _monitorTask = MonitorAndArchiveAsync(startedAt, _monitorCancellation.Token);
        }
        catch (Exception exception)
        {
            LastError = exception.Message;
            SetState(ScreenRecordingState.Failed);
            throw;
        }
        finally
        {
            _gate.Release();
        }
    }

    public void Dispose()
    {
        _monitorCancellation?.Cancel();
        _monitorCancellation?.Dispose();
        _gate.Dispose();
    }

    private async Task MonitorAndArchiveAsync(DateTimeOffset startedAt, CancellationToken cancellationToken)
    {
        var tracker = new ScreenRecordingArchiveTracker(startedAt);
        try
        {
            while (!cancellationToken.IsCancellationRequested)
            {
                await Task.Delay(TimeSpan.FromSeconds(1), cancellationToken);
                var completed = tracker.Observe(EnumerateCandidates(), DateTimeOffset.Now);
                if (completed is null || !CanOpenExclusively(completed.Path)) continue;

                SetState(ScreenRecordingState.Archiving);
                LastArchivedPath = await ArchiveAsync(completed.Path, cancellationToken);
                SetState(ScreenRecordingState.Completed);
                return;
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            SetState(ScreenRecordingState.Idle);
        }
        catch (Exception exception)
        {
            LastError = exception.Message;
            SetState(ScreenRecordingState.Failed);
        }
    }

    private static IEnumerable<ScreenRecordingFileCandidate> EnumerateCandidates()
    {
        foreach (var directory in RecordingDirectories())
        {
            if (!Directory.Exists(directory)) continue;
            IEnumerable<string> paths;
            try
            {
                paths = Directory.EnumerateFiles(directory, "*.mp4", SearchOption.TopDirectoryOnly).ToArray();
            }
            catch (IOException)
            {
                continue;
            }
            catch (UnauthorizedAccessException)
            {
                continue;
            }

            foreach (var path in paths)
            {
                FileInfo info;
                try
                {
                    info = new FileInfo(path);
                    if (!info.Exists) continue;
                }
                catch (IOException)
                {
                    continue;
                }
                yield return new(path, info.Length, info.LastWriteTimeUtc);
            }
        }
    }

    private static IEnumerable<string> RecordingDirectories()
    {
        var videos = Environment.GetFolderPath(Environment.SpecialFolder.MyVideos);
        if (string.IsNullOrWhiteSpace(videos)) yield break;
        yield return Path.Combine(videos, "Screen Recordings");
        yield return Path.Combine(videos, "Captures");
    }

    private static bool CanOpenExclusively(string path)
    {
        try
        {
            using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.None);
            return stream.Length > 0;
        }
        catch (IOException)
        {
            return false;
        }
        catch (UnauthorizedAccessException)
        {
            return false;
        }
    }

    private static async Task<string> ArchiveAsync(string sourcePath, CancellationToken cancellationToken)
    {
        var videos = Environment.GetFolderPath(Environment.SpecialFolder.MyVideos);
        var archiveDirectory = Path.Combine(videos, "ChatOS");
        Directory.CreateDirectory(archiveDirectory);
        var baseName = $"ChatOS Recording {DateTime.Now:yyyy-MM-dd HH-mm-ss}";
        var destination = Path.Combine(archiveDirectory, baseName + ".mp4");
        for (var suffix = 2; File.Exists(destination); suffix++)
            destination = Path.Combine(archiveDirectory, $"{baseName} {suffix}.mp4");

        await using var source = new FileStream(
            sourcePath, FileMode.Open, FileAccess.Read, FileShare.Read, 1024 * 1024,
            FileOptions.Asynchronous | FileOptions.SequentialScan);
        await using var target = new FileStream(
            destination, FileMode.CreateNew, FileAccess.Write, FileShare.None, 1024 * 1024,
            FileOptions.Asynchronous | FileOptions.SequentialScan);
        await source.CopyToAsync(target, cancellationToken);
        await target.FlushAsync(cancellationToken);
        return destination;
    }

    private static void LaunchNativeRecorder() => Process.Start(new ProcessStartInfo(
        "ms-screenclip:?type=recording")
    {
        UseShellExecute = true,
    });

    private void SetState(ScreenRecordingState state)
    {
        State = state;
        StateChanged?.Invoke(this, EventArgs.Empty);
    }
}
