using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Notepad;

public sealed partial class NotepadViewModel
{
    private CancellationTokenSource? _searchCancellation;
    private CancellationTokenSource? _scheduledSearchCancellation;
    private string _appliedSearchQuery = string.Empty;

    public bool IsSearchActive => _appliedSearchQuery.Length > 0;

    public bool CanSearch => IsOpen && !IsLoading && !IsSaving &&
        _sessionCancellation is { IsCancellationRequested: false };

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsBusy))]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    [NotifyPropertyChangedFor(nameof(CanReload))]
    [NotifyCanExecuteChangedFor(nameof(SaveCommand))]
    [NotifyCanExecuteChangedFor(nameof(RefreshCommand))]
    [NotifyCanExecuteChangedFor(nameof(CreateNoteCommand))]
    private bool _isSearching;

    partial void OnSearchQueryChanged(string value) => ScheduleSearchCommand.Execute(null);

    [RelayCommand(AllowConcurrentExecutions = true)]
    private async Task ScheduleSearchAsync()
    {
        CancelSearch();
        if (!CanSearch) return;
        var sessionToken = _sessionCancellation!.Token;
        var scheduled = CancellationTokenSource.CreateLinkedTokenSource(sessionToken);
        _scheduledSearchCancellation = scheduled;
        try
        {
            Task search = Task.CompletedTask;
            bool waitForEditor;
            do
            {
                await Task.Delay(260, scheduled.Token).ConfigureAwait(false);
                waitForEditor = false;
                await InvokeForSessionAsync(() =>
                {
                    if (!ReferenceEquals(_scheduledSearchCancellation, scheduled) || scheduled.IsCancellationRequested) return;
                    // A save/refresh may start during the debounce. Keep the last
                    // input queued until the editor is ready instead of losing it.
                    if (!CanSearch) { waitForEditor = true; return; }
                    _scheduledSearchCancellation = null;
                    search = SearchCommand.ExecuteAsync(null);
                }, sessionToken).ConfigureAwait(false);
            } while (waitForEditor);
            await search.ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (scheduled.IsCancellationRequested)
        {
        }
        finally
        {
            if (ReferenceEquals(_scheduledSearchCancellation, scheduled))
            {
                await InvokeForSessionAsync(() =>
                {
                    if (ReferenceEquals(_scheduledSearchCancellation, scheduled)) _scheduledSearchCancellation = null;
                }, sessionToken, allowCanceled: true).ConfigureAwait(false);
            }
            scheduled.Dispose();
        }
    }

    private void CancelScheduledSearch()
    {
        var previous = _scheduledSearchCancellation;
        _scheduledSearchCancellation = null;
        previous?.Cancel();
    }

    [RelayCommand(CanExecute = nameof(CanSearch), AllowConcurrentExecutions = true)]
    private async Task SearchAsync()
    {
        CancelScheduledSearch();
        if (!CanSearch) return;
        var sessionToken = _sessionCancellation!.Token;
        var query = SearchQuery.Trim();
        var previous = _searchCancellation;
        var search = CancellationTokenSource.CreateLinkedTokenSource(sessionToken);
        _searchCancellation = search;
        previous?.Cancel();
        // Each request owns its source until its finally block, including when the
        // service ignores cancellation and returns after a replacement request.
        IsSearching = true;
        ErrorMessage = null;
        try
        {
            var notes = await _service.ListNotesAsync(query.Length == 0 ? null : query, 500, search.Token).ConfigureAwait(false);
            await InvokeForSessionAsync(() =>
            {
                if (ReferenceEquals(_searchCancellation, search)) ApplyNoteResults(notes, query);
            }, sessionToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (search.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await InvokeForSessionAsync(() =>
            {
                if (ReferenceEquals(_searchCancellation, search)) ErrorMessage = exception.Message;
            }, sessionToken).ConfigureAwait(false);
        }
        finally
        {
            await InvokeForSessionAsync(() =>
            {
                if (!ReferenceEquals(_searchCancellation, search)) return;
                _searchCancellation = null;
                IsSearching = false;
            }, sessionToken, allowCanceled: true).ConfigureAwait(false);
            search.Dispose();
        }
    }

    [RelayCommand]
    private void CancelSearch()
    {
        CancelScheduledSearch();
        var previous = _searchCancellation;
        _searchCancellation = null;
        previous?.Cancel();
        IsSearching = false;
    }

    private void ApplyNoteResults(IReadOnlyList<NotepadNote> notes, string query)
    {
        _allNotes.Clear();
        _allNotes.AddRange(notes.OrderByDescending(static note => note.UpdatedAt ?? note.CreatedAt));
        _appliedSearchQuery = query;
        OnPropertyChanged(nameof(IsSearchActive));
        ApplyFilter();
    }
}
