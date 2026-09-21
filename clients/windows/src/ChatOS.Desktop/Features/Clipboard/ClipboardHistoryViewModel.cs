using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.Clipboard;

public sealed partial class ClipboardHistoryViewModel : ObservableObject
{
    private readonly IClipboardHistoryStore _store;
    private readonly WindowsClipboardHistoryMonitor _monitor;
    private ClipboardHistoryEntry[] _snapshot = [];
    private long _loadGeneration;

    public ClipboardHistoryViewModel(
        IClipboardHistoryStore store,
        WindowsClipboardHistoryMonitor monitor)
    {
        _store = store;
        _monitor = monitor;
    }

    public ObservableCollection<ClipboardHistoryEntry> Entries { get; } = [];

    [ObservableProperty]
    private string _query = string.Empty;

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private string? _errorMessage;

    public bool HasEntries => Entries.Count != 0;

    partial void OnQueryChanged(string value) => ApplyFilter();

    public async Task LoadAsync(CancellationToken cancellationToken = default)
    {
        var generation = Interlocked.Increment(ref _loadGeneration);
        try
        {
            IsLoading = true;
            ErrorMessage = null;
            var entries = await _store.ListAsync(cancellationToken: cancellationToken);
            if (generation != Interlocked.Read(ref _loadGeneration)) return;
            _snapshot = entries.ToArray();
            ApplyFilter();
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
        finally
        {
            if (generation == Interlocked.Read(ref _loadGeneration)) IsLoading = false;
        }
    }

    public async Task TogglePinnedAsync(ClipboardHistoryEntry entry)
    {
        await RunMutationAsync(async () =>
        {
            await _store.SetPinnedAsync(entry.Id, !entry.IsPinned);
            await LoadAsync();
        });
    }

    public async Task DeleteAsync(ClipboardHistoryEntry entry)
    {
        await RunMutationAsync(async () =>
        {
            await _store.DeleteAsync(entry.Id);
            await LoadAsync();
        });
    }

    public async Task RestoreAsync(ClipboardHistoryEntry entry)
    {
        await RunMutationAsync(async () =>
        {
            var payload = await _store.ReadPayloadAsync(entry.Id)
                ?? throw new InvalidOperationException("Clipboard item is no longer available.");
            await _monitor.RestoreAsync(payload);
        });
    }

    private void ApplyFilter()
    {
        var query = Query.Trim();
        var filtered = query.Length == 0
            ? _snapshot
            : _snapshot.Where(entry =>
                entry.Preview.Contains(query, StringComparison.CurrentCultureIgnoreCase)
                || (entry.SourceApplication?.Contains(query, StringComparison.CurrentCultureIgnoreCase) ?? false));
        Entries.Clear();
        foreach (var entry in filtered) Entries.Add(entry);
        OnPropertyChanged(nameof(HasEntries));
    }

    private async Task RunMutationAsync(Func<Task> mutation)
    {
        try
        {
            ErrorMessage = null;
            await mutation();
        }
        catch (Exception exception)
        {
            ErrorMessage = exception.Message;
        }
    }
}
