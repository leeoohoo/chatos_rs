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
    private CancellationTokenSource? _searchCancellation;
    private CancellationTokenSource? _navigationCancellation;
    private readonly List<NavigationPoint> _navigationHistory = [];
    private long _generation;
    private long _searchGeneration;
    private long _navigationGeneration;

    public event EventHandler<string>? DirectoryChanged;

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
        Entries.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(IsDirectoryEmpty));
            OnPropertyChanged(nameof(HasEmptyDirectoryState));
        };
        FileSearchResults.CollectionChanged += (_, _) => NotifySearchResultStateChanged();
        ContentSearchResults.CollectionChanged += (_, _) => NotifySearchResultStateChanged();
        NavigationLocations.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasNavigationResults));
    }

    public ObservableCollection<ProjectFileEntry> Entries { get; } = [];

    public ObservableCollection<ProjectDirectoryBreadcrumb> Breadcrumbs { get; } = [];

    public ObservableCollection<ProjectFileEntry> FileSearchResults { get; } = [];

    public ObservableCollection<ProjectFileContentMatch> ContentSearchResults { get; } = [];

    public ObservableCollection<ProjectCodeNavigationLocation> NavigationLocations { get; } = [];

    public bool IsDirectoryEmpty => Entries.Count == 0;

    public bool HasEmptyDirectoryState =>
        IsOpen &&
        !IsLoading &&
        !IsSearchActive &&
        IsDirectoryEmpty &&
        string.IsNullOrWhiteSpace(ErrorMessage);

    public bool HasSearchResults => FileSearchResults.Count > 0 || ContentSearchResults.Count > 0;

    public bool IsSearchActive => !string.IsNullOrWhiteSpace(SearchQuery);

    public bool HasNoSearchResults => IsSearchActive && !IsSearching && !HasSearchResults;

    public bool HasNavigationResults => NavigationLocations.Count > 0;

    public bool CanNavigateBack => _navigationHistory.Count > 0;

    public bool HasSelectedFile => SelectedFile is not null;

    public string SelectedFileName => SelectedFile?.Name ?? string.Empty;

    public string SelectedFileDisplayPath => SelectedFile?.DisplayPath ?? string.Empty;

    public bool CanEdit => SelectedFile is { IsBinary: false, IsWritable: true };

    public bool IsDirty =>
        IsEditing &&
        SelectedFile is not null &&
        !string.Equals(EditorContent, SelectedFile.Content, StringComparison.Ordinal);

    public bool CanSave => IsDirty && !IsSaving;

    public bool IsPreviewMode => HasSelectedFile && !IsEditing;

    public bool HasTextPreview => SelectedFile is { IsBinary: false } && !IsEditing && !HasImagePreview;

    public bool HasMarkdownPreview =>
        HasTextPreview &&
        !HasNavigationTarget &&
        SelectedFile is { } file &&
        IsMarkdown(file.Name);

    public bool HasCodeTextPreview => HasTextPreview && !HasMarkdownPreview;

    public bool HasImagePreview => SelectedFile is { } file && SupportsImagePreview(file) && !IsEditing;

    public bool HasBinaryPreview => SelectedFile is { IsBinary: true } && !HasImagePreview;

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
    [NotifyPropertyChangedFor(nameof(HasEmptyDirectoryState))]
    private bool _isOpen;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasEmptyDirectoryState))]
    private bool _isLoading;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasNoSearchResults))]
    private bool _isSearching;

    [ObservableProperty]
    private bool _isNavigating;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    private bool _isSaving;

    [ObservableProperty]
    private bool _isMutatingEntry;

    [ObservableProperty]
    private bool _isDirectoryWritable;

    [ObservableProperty]
    private bool _isTruncated;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsPreviewMode))]
    [NotifyPropertyChangedFor(nameof(HasTextPreview))]
    [NotifyPropertyChangedFor(nameof(HasMarkdownPreview))]
    [NotifyPropertyChangedFor(nameof(HasCodeTextPreview))]
    [NotifyPropertyChangedFor(nameof(HasImagePreview))]
    [NotifyPropertyChangedFor(nameof(HasBinaryPreview))]
    [NotifyPropertyChangedFor(nameof(IsDirty))]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    private bool _isEditing;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasSelectedFile))]
    [NotifyPropertyChangedFor(nameof(SelectedFileName))]
    [NotifyPropertyChangedFor(nameof(SelectedFileDisplayPath))]
    [NotifyPropertyChangedFor(nameof(CanEdit))]
    [NotifyPropertyChangedFor(nameof(IsPreviewMode))]
    [NotifyPropertyChangedFor(nameof(HasTextPreview))]
    [NotifyPropertyChangedFor(nameof(HasMarkdownPreview))]
    [NotifyPropertyChangedFor(nameof(HasCodeTextPreview))]
    [NotifyPropertyChangedFor(nameof(HasImagePreview))]
    [NotifyPropertyChangedFor(nameof(HasBinaryPreview))]
    [NotifyPropertyChangedFor(nameof(IsDirty))]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    private ProjectFileContent? _selectedFile;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsDirty))]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    private string _editorContent = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsSearchActive))]
    [NotifyPropertyChangedFor(nameof(HasNoSearchResults))]
    [NotifyPropertyChangedFor(nameof(HasEmptyDirectoryState))]
    private string _searchQuery = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasEmptyDirectoryState))]
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

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasMarkdownPreview))]
    [NotifyPropertyChangedFor(nameof(HasCodeTextPreview))]
    private bool _hasNavigationTarget;

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

    public Task CloseAsync(CancellationToken cancellationToken = default)
    {
        CancelSession();
        return _dispatcher.InvokeAsync(() => ResetClosed(), cancellationToken);
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

    public Task NavigateToPathAsync(string? path)
    {
        if (_sessionCancellation is null || string.IsNullOrWhiteSpace(path))
        {
            return Task.CompletedTask;
        }

        var normalized = path.Trim();
        if (string.Equals(normalized, CurrentPath, StringComparison.Ordinal))
        {
            return Task.CompletedTask;
        }

        return LoadDirectoryInternalAsync(
            normalized,
            false,
            Interlocked.Increment(ref _generation),
            _sessionCancellation.Token);
    }

    public async Task<ProjectDirectoryListing?> LoadTreeDirectoryAsync(
        string path,
        CancellationToken cancellationToken = default)
    {
        // Tree expansion is initiated by the view and can outlive the page that
        // started it.  Tie the request to the active project session so a late
        // response from project A cannot populate project B's tree after a
        // selection change.
        var generation = _generation;
        var sessionToken = _sessionCancellation?.Token ?? CancellationToken.None;
        using var linkedCancellation = CancellationTokenSource.CreateLinkedTokenSource(
            sessionToken,
            cancellationToken);
        var token = linkedCancellation.Token;
        try
        {
            var listing = await _filesystem.ListEntriesAsync(path, false, token)
                .ConfigureAwait(false);
            return generation == _generation && !token.IsCancellationRequested
                ? listing
                : null;
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
            return null;
        }
        catch (Exception exception)
        {
            if (generation == _generation && !token.IsCancellationRequested)
            {
                await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                    .ConfigureAwait(false);
            }
            return null;
        }
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
            _sessionCancellation.Token,
            match.Line,
            match.Column);
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

        PushNavigationPoint();
        EntryActionMessage = L(
            $"已跳转到 {location.RelativePath}:{location.Line}:{location.Column}",
            $"Opened {location.RelativePath}:{location.Line}:{location.Column}");
        return ReadFileInternalAsync(
            location.Path,
            Interlocked.Increment(ref _generation),
            _sessionCancellation.Token,
            location.Line,
            location.Column,
            preserveNavigationHistory: true);
    }

    [RelayCommand]
    private Task NavigateBackAsync()
    {
        if (_sessionCancellation is null || _navigationHistory.Count == 0)
        {
            return Task.CompletedTask;
        }

        var index = _navigationHistory.Count - 1;
        var point = _navigationHistory[index];
        _navigationHistory.RemoveAt(index);
        OnPropertyChanged(nameof(CanNavigateBack));
        EntryActionMessage = L(
            $"已返回 {point.DisplayPath}:{point.Line}:{point.Column}",
            $"Returned to {point.DisplayPath}:{point.Line}:{point.Column}");
        return ReadFileInternalAsync(
            point.Path,
            Interlocked.Increment(ref _generation),
            _sessionCancellation.Token,
            point.Line,
            point.Column,
            preserveNavigationHistory: true);
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

        CancelScheduledSearch();
        var generation = Interlocked.Increment(ref _searchGeneration);
        _searchCancellation = CancellationTokenSource.CreateLinkedTokenSource(_sessionCancellation.Token);
        await SearchCoreAsync(query, SearchRootPath, generation, _searchCancellation.Token).ConfigureAwait(false);
    }

    private string SearchRootPath => string.IsNullOrWhiteSpace(ProjectRoot) ? CurrentPath : ProjectRoot;

    private async Task SearchCoreAsync(string query, string path, long generation, CancellationToken token)
    {
        bool IsCurrentSearch() => generation == _searchGeneration &&
            !token.IsCancellationRequested &&
            string.Equals(SearchQuery.Trim(), query, StringComparison.Ordinal);

        try
        {
            var started = false;
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentSearch()) return;
                IsSearching = true;
                ErrorMessage = null;
                started = true;
            }, token).ConfigureAwait(false);
            if (!started || token.IsCancellationRequested) return;

            // Capture both invocation and task failures independently, matching
            // macOS: one unavailable provider must not discard the other's hits.
            var entriesTask = CaptureSearchAsync(() => _filesystem.SearchEntriesAsync(path, query, 100, token), token);
            var contentTask = CaptureSearchAsync(() => _filesystem.SearchContentAsync(path, query, 200, token), token);
            await Task.WhenAll(entriesTask, contentTask).ConfigureAwait(false);
            var entries = await entriesTask.ConfigureAwait(false);
            var content = await contentTask.ConfigureAwait(false);

            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentSearch())
                {
                    return;
                }

                FileSearchResults.Clear();
                foreach (var entry in entries.Values)
                {
                    FileSearchResults.Add(entry);
                }

                ContentSearchResults.Clear();
                foreach (var match in content.Values)
                {
                    ContentSearchResults.Add(match);
                }

                var errors = new List<string>(capacity: 2);
                if (entries.Error is { } entryError)
                    errors.Add(L($"文件名搜索失败：{entryError}", $"File search failed: {entryError}"));
                if (content.Error is { } contentError)
                    errors.Add(L($"内容搜索失败：{contentError}", $"Content search failed: {contentError}"));
                ErrorMessage = errors.Count == 0 ? null : string.Join("；", errors);
            }, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (IsCurrentSearch()) ErrorMessage = exception.Message;
            })
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _searchGeneration)
                {
                    IsSearching = false;
                }
            })
                .ConfigureAwait(false);
        }
    }

    private static async Task<(IReadOnlyList<T> Values, string? Error)> CaptureSearchAsync<T>(
        Func<Task<IReadOnlyList<T>>> search, CancellationToken token)
    {
        try
        {
            return (await search().ConfigureAwait(false), null);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception exception)
        {
            return ([], exception.Message);
        }
    }

    [RelayCommand]
    private void ClearSearch()
    {
        SearchQuery = string.Empty;
        FileSearchResults.Clear();
        ContentSearchResults.Clear();
    }

    partial void OnSearchQueryChanged(string value)
    {
        var generation = Interlocked.Increment(ref _searchGeneration);
        CancelScheduledSearch();
        var query = value.Trim();
        if (_sessionCancellation is null || query.Length == 0)
        {
            FileSearchResults.Clear();
            ContentSearchResults.Clear();
            IsSearching = false;
            return;
        }

        _searchCancellation = CancellationTokenSource.CreateLinkedTokenSource(_sessionCancellation.Token);
        _ = RunScheduledSearchAsync(query, SearchRootPath, generation, _searchCancellation.Token);
    }

    private void NotifySearchResultStateChanged()
    {
        OnPropertyChanged(nameof(HasSearchResults));
        OnPropertyChanged(nameof(HasNoSearchResults));
    }

    partial void OnCurrentPathChanged(string value)
    {
        RebuildBreadcrumbs(value);
    }

    partial void OnSelectedFileChanged(ProjectFileContent? value) => ClearCodeNavigation();

    partial void OnNavigationLineChanged(double value) => ClearCodeNavigation();

    partial void OnNavigationColumnChanged(double value) => ClearCodeNavigation();

    private async Task RunScheduledSearchAsync(string query, string path, long generation, CancellationToken token)
    {
        try
        {
            await Task.Delay(TimeSpan.FromMilliseconds(280), token).ConfigureAwait(false);
            await SearchCoreAsync(query, path, generation, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
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

        CancelCodeNavigation();
        _navigationCancellation = CancellationTokenSource.CreateLinkedTokenSource(_sessionCancellation.Token);
        var token = _navigationCancellation.Token;
        var generation = _navigationGeneration;
        var fileGeneration = _generation;
        var root = ProjectRoot;
        var line = (int)Math.Max(1, NavigationLine);
        var column = (int)Math.Max(1, NavigationColumn);
        bool IsCurrentNavigation() => generation == _navigationGeneration &&
            fileGeneration == _generation && !token.IsCancellationRequested &&
            ReferenceEquals(SelectedFile, file);

        try
        {
            var started = false;
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentNavigation()) return;
                IsNavigating = true;
                ErrorMessage = null;
                EntryActionMessage = null;
                NavigationLocations.Clear();
                NavigationToken = null;
                NavigationMode = null;
                started = true;
            }, token).ConfigureAwait(false);
            if (!started || token.IsCancellationRequested) return;

            var result = definitions
                ? await _navigation.DefinitionAsync(
                    root,
                    file.Path,
                    line,
                    column,
                    token).ConfigureAwait(false)
                : await _navigation.ReferencesAsync(
                    root,
                    file.Path,
                    line,
                    column,
                    token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentNavigation()) return;
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
            await _dispatcher.InvokeAsync(() =>
            {
                if (IsCurrentNavigation()) ErrorMessage = exception.Message;
            }).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _navigationGeneration) IsNavigating = false;
            }).ConfigureAwait(false);
        }
    }

    private void ClearCodeNavigation()
    {
        CancelCodeNavigation();
        IsNavigating = false;
        NavigationLocations.Clear();
        NavigationToken = null;
        NavigationMode = null;
    }

    private void CancelCodeNavigation()
    {
        Interlocked.Increment(ref _navigationGeneration);
        _navigationCancellation?.Cancel();
        _navigationCancellation?.Dispose();
        _navigationCancellation = null;
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
    private void CancelEditing()
    {
        if (SelectedFile is not null)
        {
            EditorContent = SelectedFile.Content;
        }
        IsEditing = false;
    }

    public async Task OpenSelectedExternallyAsync(
        ProjectFileExternalOpenMode mode,
        CancellationToken cancellationToken = default)
    {
        if (SelectedFile is null) return;
        ErrorMessage = null;
        try
        {
            await _filesystem.OpenExternallyAsync(
                SelectedFile.Path,
                mode,
                cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private async Task SaveAsync()
    {
        if (_sessionCancellation is null ||
            !CanSave ||
            SelectedFile is not { IsBinary: false, IsWritable: true } file)
        {
            return;
        }

        var token = _sessionCancellation.Token;
        var generation = _generation;
        var submittedContent = EditorContent;
        bool IsCurrentSave() =>
            !token.IsCancellationRequested &&
            _sessionCancellation?.Token == token &&
            generation == _generation &&
            ReferenceEquals(SelectedFile, file);

        IsSaving = true;
        ErrorMessage = null;
        try
        {
            await _filesystem.WriteFileAsync(
                file.Path,
                submittedContent,
                token).ConfigureAwait(false);
            var updated = file with
            {
                Content = submittedContent,
                Size = System.Text.Encoding.UTF8.GetByteCount(submittedContent),
                ModifiedAt = DateTimeOffset.UtcNow,
            };
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentSave()) return;

                var keepEditing = IsEditing &&
                    !string.Equals(EditorContent, submittedContent, StringComparison.Ordinal);
                SelectedFile = updated;
                if (!keepEditing)
                {
                    EditorContent = submittedContent;
                    IsEditing = false;
                }
            }).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (IsCurrentSave()) ErrorMessage = exception.Message;
            })
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
                : _filesystem.CreateFileAsync(currentPath, name, token),
            request.ParentPath);
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
        if (!TryGetEntryParentPath(sourcePath, out var parentPath)) return Task.CompletedTask;
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
            }, parentPath);
    }

    [RelayCommand]
    private Task DeleteEntryAsync(ProjectFileEntry? entry)
    {
        if (entry is null)
        {
            return Task.CompletedTask;
        }

        var path = entry.Path;
        if (!TryGetEntryParentPath(path, out var parentPath)) return Task.CompletedTask;
        return MutateEntryAsync(
            entry.IsDirectory ? L("文件夹已删除。", "Folder deleted.") : L("文件已删除。", "File deleted."),
            (_, token) => _filesystem.DeleteEntryAsync(path, entry.IsDirectory, token),
            parentPath);
    }

    private bool TryGetEntryParentPath(string path, out string parentPath)
    {
        // Connector paths are opaque: retain URI escaping, separators and drive roots.
        var trimmed = path.TrimEnd('/', '\\');
        var separator = trimmed.LastIndexOfAny(['/', '\\']);
        if (separator < 0)
        {
            parentPath = string.Empty;
            ErrorMessage = L("无法确定条目的父目录，请刷新目录后重试。",
                "The entry's parent directory could not be determined. Refresh the directory and try again.");
            return false;
        }

        var parentLength = separator == 0 || separator == 2 && trimmed[1] == ':'
            ? separator + 1
            : separator;
        parentPath = trimmed[..parentLength];
        return true;
    }

    public void Dispose() => CancelSession();

    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;

    private async Task MutateEntryAsync(
        string successMessage,
        Func<string, CancellationToken, Task> mutation,
        string? targetPath = null)
    {
        if (_sessionCancellation is null ||
            string.IsNullOrWhiteSpace(targetPath ?? CurrentPath) ||
            IsMutatingEntry)
        {
            return;
        }

        var currentPath = targetPath ?? CurrentPath;
        var token = _sessionCancellation.Token;
        var generation = Interlocked.Increment(ref _generation);
        IsMutatingEntry = true;
        ErrorMessage = null;
        EntryActionMessage = null;
        try
        {
            await mutation(currentPath, token).ConfigureAwait(false);
            // The write has completed even if the following directory read fails.
            await _dispatcher.InvokeAsync(() =>
            {
                if (IsCurrentFileOperation(generation, token)) EntryActionMessage = successMessage;
            }, token).ConfigureAwait(false);
            if (!IsCurrentFileOperation(generation, token)) return;

            if (string.Equals(currentPath, CurrentPath, StringComparison.Ordinal))
            {
                await LoadDirectoryInternalAsync(currentPath, true, generation, token,
                    L("操作已完成，但目录刷新失败。请刷新目录，无需重复操作。",
                        "The operation completed, but the directory could not be refreshed. Refresh the directory without repeating the operation."))
                    .ConfigureAwait(false);
            }
            else
            {
                await _dispatcher.InvokeAsync(() =>
                {
                    if (IsCurrentFileOperation(generation, token)) DirectoryChanged?.Invoke(this, currentPath);
                }, token)
                    .ConfigureAwait(false);
            }
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
                // Entry actions are serialized within a project session. Navigation
                // invalidates their page results, but must still release the busy flag.
                // A new session owns its own flag and must not be changed here.
                if (_sessionCancellation?.Token == token)
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
        CancellationToken cancellationToken,
        string? refreshFailureMessage = null)
    {
        try
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentFileOperation(generation, cancellationToken)) return;
                ClearCodeNavigation();
                IsLoading = true;
                ErrorMessage = null;
            }, cancellationToken).ConfigureAwait(false);
            if (generation != _generation || cancellationToken.IsCancellationRequested) return;

            var listing = await _filesystem.ListEntriesAsync(path, forceRefresh, cancellationToken)
                .ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentFileOperation(generation, cancellationToken))
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
                NavigationLine = 1;
                NavigationColumn = 1;
                HasNavigationTarget = false;
                ClearNavigationHistory();
            }, cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (IsCurrentFileOperation(generation, cancellationToken))
                {
                    ErrorMessage = refreshFailureMessage is null
                        ? exception.Message
                        : $"{refreshFailureMessage} {exception.Message}";
                }
            })
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation && _sessionCancellation?.Token == cancellationToken)
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
        CancellationToken cancellationToken,
        int? targetLine = null,
        int? targetColumn = null,
        bool preserveNavigationHistory = false)
    {
        try
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentFileOperation(generation, cancellationToken)) return;
                ClearCodeNavigation();
                IsLoading = true;
                ErrorMessage = null;
            }, cancellationToken).ConfigureAwait(false);
            if (generation != _generation || cancellationToken.IsCancellationRequested) return;

            var file = await _filesystem.ReadFileAsync(path, cancellationToken).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentFileOperation(generation, cancellationToken))
                {
                    return;
                }

                SelectedFile = file;
                EditorContent = file.Content;
                IsEditing = false;
                NavigationLocations.Clear();
                NavigationToken = null;
                NavigationMode = null;
                NavigationLine = Math.Max(1, targetLine ?? 1);
                NavigationColumn = Math.Max(1, targetColumn ?? 1);
                HasNavigationTarget = targetLine.HasValue || targetColumn.HasValue;
                if (!preserveNavigationHistory)
                {
                    ClearNavigationHistory();
                }
            }, cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (IsCurrentFileOperation(generation, cancellationToken)) ErrorMessage = exception.Message;
            })
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation && _sessionCancellation?.Token == cancellationToken)
                {
                    IsLoading = false;
                }
            })
                .ConfigureAwait(false);
        }
    }

    // Evaluate again inside dispatched callbacks: the project can change while
    // a connector response or its UI callback is waiting to run.
    private bool IsCurrentFileOperation(long generation, CancellationToken token) =>
        generation == _generation &&
        !token.IsCancellationRequested &&
        _sessionCancellation?.Token == token;

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
        IsNavigating = false;
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
        Breadcrumbs.Clear();
        FileSearchResults.Clear();
        ContentSearchResults.Clear();
        NavigationLocations.Clear();
        NavigationToken = null;
        NavigationMode = null;
        NavigationLine = 1;
        NavigationColumn = 1;
        HasNavigationTarget = false;
        ClearNavigationHistory();
    }

    private void ResetClosed()
    {
        ProjectId = null;
        ProjectName = string.Empty;
        ProjectRoot = null;
        CurrentPath = string.Empty;
        ParentPath = null;
        IsOpen = false;
        IsLoading = false;
        IsSearching = false;
        IsNavigating = false;
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
        Breadcrumbs.Clear();
        FileSearchResults.Clear();
        ContentSearchResults.Clear();
        NavigationLocations.Clear();
        NavigationToken = null;
        NavigationMode = null;
        NavigationLine = 1;
        NavigationColumn = 1;
        HasNavigationTarget = false;
        ClearNavigationHistory();
    }

    private void CancelSession()
    {
        CancelCodeNavigation();
        Interlocked.Increment(ref _searchGeneration);
        CancelScheduledSearch();
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = null;
    }

    private void PushNavigationPoint()
    {
        if (SelectedFile is not { } file)
        {
            return;
        }

        var point = new NavigationPoint(
            file.Path,
            file.DisplayPath ?? file.Name,
            (int)Math.Max(1, NavigationLine),
            (int)Math.Max(1, NavigationColumn));
        if (_navigationHistory.Count == 0 || _navigationHistory[^1] != point)
        {
            _navigationHistory.Add(point);
            if (_navigationHistory.Count > 128)
            {
                _navigationHistory.RemoveAt(0);
            }
            OnPropertyChanged(nameof(CanNavigateBack));
        }
    }

    private void ClearNavigationHistory()
    {
        if (_navigationHistory.Count == 0)
        {
            return;
        }
        _navigationHistory.Clear();
        OnPropertyChanged(nameof(CanNavigateBack));
    }

    private void CancelScheduledSearch()
    {
        _searchCancellation?.Cancel();
        _searchCancellation?.Dispose();
        _searchCancellation = null;
    }

    private void RebuildBreadcrumbs(string path)
    {
        Breadcrumbs.Clear();
        if (string.IsNullOrWhiteSpace(path))
        {
            return;
        }

        var root = ProjectRoot?.TrimEnd('/', '\\');
        if (string.IsNullOrWhiteSpace(root))
        {
            Breadcrumbs.Add(new ProjectDirectoryBreadcrumb(path, path));
            return;
        }

        Breadcrumbs.Add(new ProjectDirectoryBreadcrumb(
            BreadcrumbName(root, isRoot: true),
            root));
        if (string.Equals(path.TrimEnd('/', '\\'), root, StringComparison.OrdinalIgnoreCase))
        {
            return;
        }

        var relative = path.StartsWith(root + "/", StringComparison.OrdinalIgnoreCase)
            ? path[(root.Length + 1)..]
            : path.StartsWith(root + "\\", StringComparison.OrdinalIgnoreCase)
                ? path[(root.Length + 1)..]
                : null;
        if (string.IsNullOrWhiteSpace(relative))
        {
            return;
        }

        var current = root;
        foreach (var part in relative.Split(['/', '\\'], StringSplitOptions.RemoveEmptyEntries))
        {
            current = current.EndsWith("/", StringComparison.Ordinal) ||
                      current.EndsWith("\\", StringComparison.Ordinal)
                ? current + part
                : current + "/" + part;
            Breadcrumbs.Add(new ProjectDirectoryBreadcrumb(part, current));
        }
    }

    private string BreadcrumbName(string path, bool isRoot)
    {
        var name = path.TrimEnd('/', '\\');
        if (isRoot && name.Length == 0)
        {
            return L("项目根目录", "Project root");
        }

        var separator = name.LastIndexOfAny(['/', '\\']);
        return separator >= 0 && separator < name.Length - 1
            ? name[(separator + 1)..]
            : name;
    }

    private static bool IsMarkdown(string name)
    {
        var extension = Path.GetExtension(name);
        return extension.Equals(".md", StringComparison.OrdinalIgnoreCase) ||
               extension.Equals(".markdown", StringComparison.OrdinalIgnoreCase);
    }

    private sealed record NavigationPoint(
        string Path,
        string DisplayPath,
        int Line,
        int Column);

    private static bool TryNormalizeEntryName(string value, out string name)
    {
        name = value.Trim();
        return name.Length > 0 &&
            name is not "." and not ".." &&
            name.IndexOfAny(['/', '\\', '\0']) < 0;
    }

    private static bool SupportsImagePreview(ProjectFileContent file)
    {
        if (file.ContentType?.StartsWith("image/", StringComparison.OrdinalIgnoreCase) == true)
        {
            return true;
        }

        return Path.GetExtension(file.Name).ToLowerInvariant() is
            ".bmp" or ".gif" or ".ico" or ".jpeg" or ".jpg" or ".png" or
            ".tif" or ".tiff" or ".webp";
    }
}

public sealed record ProjectFileCreationRequest(string Name, bool IsDirectory, string? ParentPath = null);

public sealed record ProjectDirectoryBreadcrumb(string Name, string Path);

public sealed record ProjectFileRenameRequest(ProjectFileEntry Entry, string NewName);
