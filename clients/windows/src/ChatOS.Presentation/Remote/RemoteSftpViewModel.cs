using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Remote;

public sealed record RemoteSftpRenameRequest(RemoteFileEntry Entry, string DestinationPath);
public sealed record RemoteSftpDeleteRequest(RemoteFileEntry Entry, bool Recursive);

public sealed partial class RemoteSftpViewModel : ObservableObject
{
    private readonly IRemoteSftpService _service;
    private readonly IUiDispatcher _dispatcher;
    private readonly LocalizationViewModel? _localization;

    public RemoteSftpViewModel(IRemoteSftpService service, IUiDispatcher dispatcher, LocalizationViewModel? localization = null)
    { _service = service; _dispatcher = dispatcher; _localization = localization; }

    public ObservableCollection<RemoteFileEntry> Entries { get; } = [];
    public bool HasConnection => !string.IsNullOrWhiteSpace(ConnectionId);
    public bool HasVerificationChallenge => !string.IsNullOrWhiteSpace(VerificationPrompt);

    [ObservableProperty] [NotifyPropertyChangedFor(nameof(HasConnection))] private string? _connectionId;
    [ObservableProperty] private string _connectionName = string.Empty;
    [ObservableProperty] private string _currentPath = ".";
    [ObservableProperty] private RemoteFileEntry? _selectedEntry;
    [ObservableProperty] private string _previewText = string.Empty;
    [ObservableProperty] private bool _isBusy;
    [ObservableProperty] private string? _errorMessage;
    [ObservableProperty] private string? _actionMessage;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(HasVerificationChallenge))] private string? _verificationPrompt;
    [ObservableProperty] private string _verificationCode = string.Empty;

    public async Task OpenAsync(RemoteConnection connection, CancellationToken cancellationToken = default)
    {
        ConnectionId = connection.Id;
        ConnectionName = connection.Name;
        CurrentPath = string.IsNullOrWhiteSpace(connection.DefaultRemotePath) ? "." : connection.DefaultRemotePath;
        PreviewText = string.Empty;
        await RefreshCoreAsync(cancellationToken).ConfigureAwait(false);
    }

    [RelayCommand] private Task RefreshAsync() => RefreshCoreAsync(CancellationToken.None);

    [RelayCommand]
    private async Task OpenEntryAsync(RemoteFileEntry? entry)
    {
        if (entry is null) return;
        if (entry.IsDirectory && !entry.IsSymbolicLink)
        {
            CurrentPath = entry.FullPath;
            PreviewText = string.Empty;
            await RefreshCoreAsync(CancellationToken.None).ConfigureAwait(false);
        }
        else await PreviewAsync(entry).ConfigureAwait(false);
    }

    [RelayCommand]
    private async Task UpAsync()
    {
        CurrentPath = Parent(CurrentPath);
        PreviewText = string.Empty;
        await RefreshCoreAsync(CancellationToken.None).ConfigureAwait(false);
    }

    [RelayCommand]
    private Task CreateDirectoryAsync(string? path) => string.IsNullOrWhiteSpace(path)
        ? Task.CompletedTask
        : MutateAndRefreshAsync(token => _service.CreateDirectoryAsync(ConnectionId!, Resolve(path), VerificationCode, token));

    [RelayCommand]
    private Task RenameAsync(RemoteSftpRenameRequest? request) => request is null
        ? Task.CompletedTask
        : MutateAndRefreshAsync(token => _service.RenameAsync(ConnectionId!, request.Entry.FullPath, Resolve(request.DestinationPath), VerificationCode, token));

    [RelayCommand]
    private Task DeleteAsync(RemoteSftpDeleteRequest? request) => request is null
        ? Task.CompletedTask
        : MutateAndRefreshAsync(token => _service.DeleteAsync(ConnectionId!, request.Entry.FullPath, request.Recursive, VerificationCode, token));

    public async Task DownloadAsync(RemoteFileEntry entry, Stream destination, CancellationToken cancellationToken = default) =>
        await ExecuteAsync(token => _service.DownloadAsync(ConnectionId!, entry.FullPath, destination, VerificationCode, token), cancellationToken).ConfigureAwait(false);

    public async Task UploadAsync(Stream source, string fileName, bool overwrite, CancellationToken cancellationToken = default)
    {
        await ExecuteAsync(token => _service.UploadAsync(ConnectionId!, source, Resolve(fileName), overwrite, VerificationCode, token), cancellationToken).ConfigureAwait(false);
        if (ErrorMessage is null) await RefreshCoreAsync(cancellationToken).ConfigureAwait(false);
    }

    private async Task PreviewAsync(RemoteFileEntry entry)
    {
        SelectedEntry = entry;
        await ExecuteAsync(async token =>
        {
            var text = await _service.ReadTextAsync(ConnectionId!, entry.FullPath, VerificationCode, token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => PreviewText = text, token).ConfigureAwait(false);
        }, CancellationToken.None).ConfigureAwait(false);
    }

    private async Task RefreshCoreAsync(CancellationToken token)
    {
        if (!HasConnection) return;
        await ExecuteAsync(async inner =>
        {
            var values = await _service.ListAsync(ConnectionId!, CurrentPath, VerificationCode, inner).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                Entries.Clear();
                foreach (var value in values) Entries.Add(value);
                ActionMessage = L(
                    $"已加载 {values.Count} 个条目。",
                    $"Loaded {values.Count} item{(values.Count == 1 ? string.Empty : "s")}." );
            }, inner).ConfigureAwait(false);
        }, token).ConfigureAwait(false);
    }

    private async Task MutateAndRefreshAsync(Func<CancellationToken, Task> action)
    {
        await ExecuteAsync(action, CancellationToken.None).ConfigureAwait(false);
        if (ErrorMessage is null) await RefreshCoreAsync(CancellationToken.None).ConfigureAwait(false);
    }

    private async Task ExecuteAsync(Func<CancellationToken, Task> action, CancellationToken token)
    {
        if (IsBusy) return;
        IsBusy = true;
        ErrorMessage = ActionMessage = VerificationPrompt = null;
        try { await action(token).ConfigureAwait(false); }
        catch (RemoteVerificationRequiredException challenge)
        { await _dispatcher.InvokeAsync(() => VerificationPrompt = challenge.Prompt).ConfigureAwait(false); }
        catch (Exception exception) when (exception is not OperationCanceledException)
        { await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false); }
        finally { await _dispatcher.InvokeAsync(() => IsBusy = false).ConfigureAwait(false); }
    }

    private string Resolve(string path) => path.StartsWith('/') || path.StartsWith('~') ? path.Trim() : CurrentPath.TrimEnd('/') + "/" + path.Trim();
    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;
    private static string Parent(string path)
    {
        var clean = path.TrimEnd('/');
        var index = clean.LastIndexOf('/');
        if (index < 0) return ".";
        return index == 0 ? "/" : clean[..index];
    }
}
