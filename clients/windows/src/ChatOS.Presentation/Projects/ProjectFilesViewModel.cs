using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Projects;

public sealed partial class ProjectFilesViewModel : ObservableObject, IDisposable
{
    private readonly IProjectFilesystemService _filesystem;
    private readonly IUiDispatcher _dispatcher;
    private readonly IProjectCodeNavigationService? _navigation;
    private readonly LocalizationViewModel? _localization;
    private CancellationTokenSource? _sessionCancellation;
    private long _generation;

    public ProjectFilesViewModel(
        IProjectFilesystemService filesystem,
        IUiDispatcher dispatcher,
        IProjectCodeNavigationService? navigation = null,
        LocalizationViewModel? localization = null)
    {
        _filesystem = filesystem;
        _dispatcher = dispatcher;
        _navigation = navigation;
        _localization = localization;
        Entries.CollectionChanged += (_, _) => OnPropertyChanged(nameof(IsDirectoryEmpty));
        FileSearchResults.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasSearchResults));
        ContentSearchResults.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasSearchResults));
        NavigationLocations.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasNavigationResults));
    }

    public ObservableCollection<ProjectFileEntry> Entries { get; } = [];

    public ObservableCollection<ProjectFileEntry> FileSearchResults { get; } = [];

    public ObservableCollection<ProjectFileContentMatch> ContentSearchResults { get; } = [];

    public ObservableCollection<ProjectCodeNavigationLocation> NavigationLocations { get; } = [];

    public bool IsDirectoryEmpty => Entries.Count == 0;

    public bool HasSearchResults => FileSearchResults.Count > 0 || ContentSearchResults.Count > 0;

    public bool HasNavigationResults => NavigationLocations.Count > 0;

    public bool HasSelectedFile => SelectedFile is not null;

    public bool CanEdit => SelectedFile is { IsBinary: false, IsWritable: true };

    public bool IsPreviewMode => HasSelectedFile && !IsEditing;

    public bool HasTextPreview => SelectedFile is { IsBinary: false } && !IsEditing;

    public bool HasBinaryPreview => SelectedFile is { IsBinary: true };

    [ObservableProperty]
    private string? _projectId;

    [ObservableProperty]
    private string _projectName = string.Empty;

    [ObservableProperty]
    private string? _projectRoot;

    [ObservableProperty]
    private string _currentPath = string.Empty;

    [ObservableProperty]
    private string? _parentPath;

    [ObservableProperty]
    private bool _isOpen;

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private bool _isSearching;

    [ObservableProperty]
    private bool _isSaving;

    [ObservableProperty]
    private bool _isMutatingEntry;

    [ObservableProperty]
    private bool _isDirectoryWritable;

    [ObservableProperty]
    private bool _isTruncated;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsPreviewMode))]
    private bool _isEditing;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasSelectedFile))]
    [NotifyPropertyChangedFor(nameof(CanEdit))]
    [NotifyPropertyChangedFor(nameof(IsPreviewMode))]
    [NotifyPropertyChangedFor(nameof(HasTextPreview))]
    [NotifyPropertyChangedFor(nameof(HasBinaryPreview))]
    private ProjectFileContent? _selectedFile;

    [ObservableProperty]
    private string _editorContent = string.Empty;

    [ObservableProperty]
    private string _searchQuery = string.Empty;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _entryActionMessage;

    [ObservableProperty]
    private double _navigationLine = 1;

    [ObservableProperty]
    private double _navigationColumn = 1;

    [ObservableProperty]
    private string? _navigationToken;

    [ObservableProperty]
    private string? _navigationMode;

    public async Task OpenAsync(
        WorkspaceProject project,
        CancellationToken cancellationToken = default)
    {
        CancelSession();
        _sessionCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var token = _sessionCancellation.Token;
        var generation = Interlocked.Increment(ref _generation);
        await _dispatcher.InvokeAsync(() => Reset(project), token);
        if (string.IsNullOrWhiteSpace(project.RootPath))
        {
            await _dispatcher.InvokeAsync(() =>
            {
                ErrorMessage = L("这个项目没有可访问的工作区路径。", "This project has no accessible workspace path.");
                IsLoading = false;
            }, token);
            return;
        }

        await LoadDirectoryInternalAsync(project.RootPath, false, generation, token)
            .ConfigureAwait(false);
    }

    [RelayCommand]
    private Task RefreshAsync()
    {
        if (_sessionCancellation is null || string.IsNullOrWhiteSpace(CurrentPath))
        {
            return Task.CompletedTask;
        }

        return LoadDirectoryInternalAsync(
            CurrentPath,
            true,
            Interlocked.Increment(ref _generation),
            _sessionCancellation.Token);
    }

    [RelayCommand]
    private Task GoToParentAsync()
    {
        if (_sessionCancellation is null || string.IsNullOrWhiteSpace(ParentPath))
        {
            return Task.CompletedTask;
        }

        return LoadDirectoryInternalAsync(
            ParentPath,
            false,
            Interlocked.Increment(ref _generation),
            _sessionCancellation.Token);
    }

    [RelayCommand]
    private async Task OpenEntryAsync(ProjectFileEntry? entry)
    {
        if (entry is null || _sessionCancellation is null)
        {
            return;
        }

        var generation = Interlocked.Increment(ref _generation);
        if (entry.IsDirectory)
        {
            await LoadDirectoryInternalAsync(
                entry.Path,
                false,
                generation,
                _sessionCancellation.Token).ConfigureAwait(false);
            return;
        }

        await ReadFileInternalAsync(entry.Path, generation, _sessionCancellation.Token)
            .ConfigureAwait(false);
    }

    [RelayCommand]
    private Task OpenContentMatchAsync(ProjectFileContentMatch? match)
    {
        if (match is null || _sessionCancellation is null)
        {
            return Task.CompletedTask;
        }

        return ReadFileInternalAsync(
            match.Path,
            Interlocked.Increment(ref _generation),
            _sessionCancellation.Token);
    }

    [RelayCommand]
    private Task FindDefinitionAsync() => NavigateAsync(definitions: true);

    [RelayCommand]
    private Task FindReferencesAsync() => NavigateAsync(definitions: false);

    [RelayCommand]
    private Task OpenNavigationLocationAsync(ProjectCodeNavigationLocation? location)
    {
        if (location is null || _sessionCancellation is null)
        {
            return Task.CompletedTask;
        }

        NavigationLine = location.Line;
        NavigationColumn = location.Column;
        EntryActionMessage = L(
            $"已跳转到 {location.RelativePath}:{location.Line}:{location.Column}",
            $"Opened {location.RelativePath}:{location.Line}:{location.Column}");
        return ReadFileInternalAsync(
            location.Path,
            Interlocked.Increment(ref _generation),
            _sessionCancellation.Token);
    }

    [RelayCommand]
    private async Task SearchAsync()
    {
        if (_sessionCancellation is null || string.IsNullOrWhiteSpace(CurrentPath))
        {
            return;
        }

        var query = SearchQuery.Trim();
        if (query.Length == 0)
        {
            ClearSearch();
            return;
        }

        var token = _sessionCancellation.Token;
        var generation = Interlocked.Increment(ref _generation);
        IsSearching = true;
        ErrorMessage = null;
        try
        {
            var entriesTask = _filesystem.SearchEntriesAsync(CurrentPath, query, 100, token);
            var contentTask = _filesystem.SearchContentAsync(CurrentPath, query, 100, token);
            await Task.WhenAll(entriesTask, contentTask).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation != _generation)
                {
                    return;
                }

                FileSearchResults.Clear();
                foreach (var entry in entriesTask.Result)
                {
                    FileSearchResults.Add(entry);
                }

                ContentSearchResults.Clear();
                foreach (var match in contentTask.Result)
                {
                    ContentSearchResults.Add(match);
                }
            }, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation)
                {
                    IsSearching = false;
                }
            })
                .ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private void ClearSearch()
    {
        SearchQuery = string.Empty;
        FileSearchResults.Clear();
        ContentSearchResults.Clear();
    }

    private async Task NavigateAsync(bool definitions)
    {
        if (_navigation is null ||
            _sessionCancellation is null ||
            string.IsNullOrWhiteSpace(ProjectRoot) ||
            SelectedFile is not { IsBinary: false } file)
        {
            ErrorMessage = _navigation is null
                ? L("当前环境没有启用本机代码导航。", "Local code navigation is not enabled in this environment.")
                : L("请先选择一个文本文件。", "Select a text file first.");
            return;
        }

        var token = _sessionCancellation.Token;
        IsSearching = true;
        ErrorMessage = null;
        EntryActionMessage = null;
        try
        {
            var result = definitions
                ? await _navigation.DefinitionAsync(
                    ProjectRoot,
                    file.Path,
                    (int)Math.Max(1, NavigationLine),
                    (int)Math.Max(1, NavigationColumn),
                    token).ConfigureAwait(false)
                : await _navigation.ReferencesAsync(
                    ProjectRoot,
                    file.Path,
                    (int)Math.Max(1, NavigationLine),
                    (int)Math.Max(1, NavigationColumn),
                    token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                NavigationToken = result.Token;
                NavigationMode = definitions ? L("定义", "Definitions") : L("引用", "References");
                NavigationLocations.Clear();
                foreach (var location in result.Locations)
                {
                    NavigationLocations.Add(location);
                }

                EntryActionMessage = result.Token is null
                    ? L("指定位置没有可导航的符号。", "No navigable symbol was found at the selected location.")
                    : L(
                        $"{result.Token} · 找到 {result.Locations.Count} 个{NavigationMode}结果",
                        $"{result.Token} · {result.Locations.Count} {NavigationMode.ToLowerInvariant()} result{(result.Locations.Count == 1 ? string.Empty : "s")}");
            }, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsSearching = false).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private void ToggleEditing()
    {
        if (!CanEdit)
        {
            return;
        }

        IsEditing = !IsEditing;
        if (!IsEditing && SelectedFile is not null)
        {
            EditorContent = SelectedFile.Content;
        }
    }

    [RelayCommand]
    private async Task SaveAsync()
    {
        if (_sessionCancellation is null || SelectedFile is not { IsBinary: false, IsWritable: true } file)
        {
            return;
        }

        var token = _sessionCancellation.Token;
        IsSaving = true;
        ErrorMessage = null;
        try
        {
            await _filesystem.WriteFileAsync(
                file.Path,
                EditorContent,
                token).ConfigureAwait(false);
            var updated = file with
            {
                Content = EditorContent,
                Size = System.Text.Encoding.UTF8.GetByteCount(EditorContent),
                ModifiedAt = DateTimeOffset.UtcNow,
            };
            await _dispatcher.InvokeAsync(() =>
            {
                SelectedFile = updated;
                IsEditing = false;
            }).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (_sessionCancellation?.Token == token)
                {
                    IsSaving = false;
                }
            })
                .ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private Task CreateEntryAsync(ProjectFileCreationRequest? request)
    {
        if (request is null || !TryNormalizeEntryName(request.Name, out var name))
        {
            ErrorMessage = L("名称不能为空，也不能包含 / 或 \\。", "The name cannot be empty or contain / or \\.");
            return Task.CompletedTask;
        }

        return MutateEntryAsync(
            request.IsDirectory ? L("文件夹已创建。", "Folder created.") : L("文件已创建。", "File created."),
            (currentPath, token) => request.IsDirectory
                ? _filesystem.CreateDirectoryAsync(currentPath, name, token)
                : _filesystem.CreateFileAsync(currentPath, name, token));
    }

    [RelayCommand]
    private Task RenameEntryAsync(ProjectFileRenameRequest? request)
    {
        if (request is null || !TryNormalizeEntryName(request.NewName, out var name))
        {
            ErrorMessage = L("名称不能为空，也不能包含 / 或 \\。", "The name cannot be empty or contain / or \\.");
            return Task.CompletedTask;
        }

        if (string.Equals(request.Entry.Name, name, StringComparison.Ordinal))
        {
            return Task.CompletedTask;
        }

        var sourcePath = request.Entry.Path;
        return MutateEntryAsync(
            L("名称已更新。", "Name updated."),
            async (currentPath, token) =>
            {
                _ = await _filesystem.MoveEntryAsync(
                    sourcePath,
                    currentPath,
                    name,
                    false,
                    token).ConfigureAwait(false);
            });
    }

    [RelayCommand]
    private Task DeleteEntryAsync(ProjectFileEntry? entry)
    {
        if (entry is null)
        {
            return Task.CompletedTask;
        }

        var path = entry.Path;
        return MutateEntryAsync(
            entry.IsDirectory ? L("文件夹已删除。", "Folder deleted.") : L("文件已删除。", "File deleted."),
            (_, token) => _filesystem.DeleteEntryAsync(path, entry.IsDirectory, token));
    }

    public void Dispose() => CancelSession();

    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;

    private async Task MutateEntryAsync(
        string successMessage,
        Func<string, CancellationToken, Task> mutation)
    {
        if (_sessionCancellation is null ||
            string.IsNullOrWhiteSpace(CurrentPath) ||
            IsMutatingEntry)
        {
            return;
        }

        var currentPath = CurrentPath;
        var token = _sessionCancellation.Token;
        var generation = Interlocked.Increment(ref _generation);
        IsMutatingEntry = true;
        ErrorMessage = null;
        EntryActionMessage = null;
        try
        {
            await mutation(currentPath, token).ConfigureAwait(false);
            await LoadDirectoryInternalAsync(currentPath, true, generation, token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation &&
                    _sessionCancellation?.Token == token &&
                    ErrorMessage is null)
                {
                    EntryActionMessage = successMessage;
                }
            }, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation && _sessionCancellation?.Token == token)
                {
                    ErrorMessage = exception.Message;
                }
            }).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation && _sessionCancellation?.Token == token)
                {
                    IsMutatingEntry = false;
                }
            }).ConfigureAwait(false);
        }
    }

    private async Task LoadDirectoryInternalAsync(
        string path,
        bool forceRefresh,
        long generation,
        CancellationToken cancellationToken)
    {
        await _dispatcher.InvokeAsync(() =>
        {
            IsLoading = true;
            ErrorMessage = null;
        }, cancellationToken).ConfigureAwait(false);
        try
        {
            var listing = await _filesystem.ListEntriesAsync(path, forceRefresh, cancellationToken)
                .ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation != _generation)
                {
                    return;
                }

                CurrentPath = listing.Path;
                ParentPath = listing.ParentPath;
                IsDirectoryWritable = listing.IsWritable;
                IsTruncated = listing.IsTruncated;
                Entries.Clear();
                foreach (var entry in listing.Entries
                             .OrderByDescending(static value => value.IsDirectory)
                             .ThenBy(static value => value.Name, StringComparer.CurrentCultureIgnoreCase))
                {
                    Entries.Add(entry);
                }

                SelectedFile = null;
                EditorContent = string.Empty;
                IsEditing = false;
                ClearSearch();
                NavigationLocations.Clear();
                NavigationToken = null;
                NavigationMode = null;
            }, cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation)
                {
                    IsLoading = false;
                }
            })
                .ConfigureAwait(false);
        }
    }

    private async Task ReadFileInternalAsync(
        string path,
        long generation,
        CancellationToken cancellationToken)
    {
        await _dispatcher.InvokeAsync(() =>
        {
            IsLoading = true;
            ErrorMessage = null;
        }, cancellationToken).ConfigureAwait(false);
        try
        {
            var file = await _filesystem.ReadFileAsync(path, cancellationToken).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation != _generation)
                {
                    return;
                }

                SelectedFile = file;
                EditorContent = file.Content;
                IsEditing = false;
                NavigationLocations.Clear();
                NavigationToken = null;
                NavigationMode = null;
                NavigationLine = 1;
                NavigationColumn = 1;
            }, cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation)
                {
                    IsLoading = false;
                }
            })
                .ConfigureAwait(false);
        }
    }

    private void Reset(WorkspaceProject project)
    {
        ProjectId = project.Id;
        ProjectName = project.Name;
        ProjectRoot = project.RootPath;
        CurrentPath = project.RootPath ?? string.Empty;
        ParentPath = null;
        IsOpen = true;
        IsLoading = true;
        IsSearching = false;
        IsSaving = false;
        IsMutatingEntry = false;
        IsDirectoryWritable = false;
        IsTruncated = false;
        IsEditing = false;
        SelectedFile = null;
        EditorContent = string.Empty;
        SearchQuery = string.Empty;
        ErrorMessage = null;
        EntryActionMessage = null;
        Entries.Clear();
        FileSearchResults.Clear();
        ContentSearchResults.Clear();
        NavigationLocations.Clear();
        NavigationToken = null;
        NavigationMode = null;
        NavigationLine = 1;
        NavigationColumn = 1;
    }

    private void CancelSession()
    {
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = null;
    }

    private static bool TryNormalizeEntryName(string value, out string name)
    {
        name = value.Trim();
        return name.Length > 0 &&
            name is not "." and not ".." &&
            name.IndexOfAny(['/', '\\', '\0']) < 0;
    }
}

public sealed record ProjectFileCreationRequest(string Name, bool IsDirectory);

public sealed record ProjectFileRenameRequest(ProjectFileEntry Entry, string NewName);
