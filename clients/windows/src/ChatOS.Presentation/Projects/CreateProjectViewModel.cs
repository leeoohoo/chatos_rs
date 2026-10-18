using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Presentation.Projects;

public sealed partial class CreateProjectViewModel : ObservableObject, IDisposable
{
    private static readonly HashSet<string> WindowsRootSystemDirectories = new(
        [
            "$Recycle.Bin", "Documents and Settings", "PerfLogs", "ProgramData",
            "Recovery", "System Volume Information", "Windows",
        ],
        StringComparer.OrdinalIgnoreCase);

    private static readonly HashSet<string> WindowsUserSystemDirectories = new(
        ["AppData"],
        StringComparer.OrdinalIgnoreCase);

    private readonly IProjectFilesystemService _filesystem;
    private readonly LocalizationViewModel? _localization;
    private readonly List<ProjectFileEntry> _allDirectoryEntries = [];
    private bool _userEditedProjectName;
    private CancellationTokenSource? _directoryCancellation;
    private long _directoryGeneration;
    private bool _disposed;

    public CreateProjectViewModel(
        LocalConnectorStatus? connectorStatus,
        IProjectFilesystemService filesystem,
        LocalizationViewModel? localization = null,
        string suggestedName = "",
        string suggestedDescription = "")
    {
        _filesystem = filesystem;
        _localization = localization;
        Workspaces = connectorStatus?.Workspaces ?? [];
        ProjectTypes = LocalProjectTypeCatalog.All
            .Select(value => new ProjectTypeOption(
                value.Key,
                localization?.Text(value.Label, value.LabelEnglish) ?? value.Label,
                localization?.Text(value.Description, value.DescriptionEnglish) ?? value.Description,
                localization?.Text(value.CategoryLabel, value.CategoryLabelEnglish) ?? value.CategoryLabel))
            .ToArray();
        SelectedProjectType = ProjectTypes.First(value =>
            value.Key == LocalProjectTypeCatalog.LegacyProjectTypeKey);
        SelectedWorkspace = Workspaces.FirstOrDefault();
        ProjectName = suggestedName;
        ProjectDescription = suggestedDescription;
        _userEditedProjectName = suggestedName.Length > 0;
    }

    public IReadOnlyList<LocalConnectorWorkspaceStatus> Workspaces { get; }

    public IReadOnlyList<ProjectTypeOption> ProjectTypes { get; }

    public ObservableCollection<ProjectFileEntry> Entries { get; } = [];

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCreate))]
    [NotifyPropertyChangedFor(nameof(CanGoToParent))]
    [NotifyPropertyChangedFor(nameof(DisplayedLocation))]
    private LocalConnectorWorkspaceStatus? _selectedWorkspace;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCreate))]
    [NotifyPropertyChangedFor(nameof(CanGoToParent))]
    private string _currentPath = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(DisplayedLocation))]
    private string? _currentRelativePath;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGoToParent))]
    private string? _parentPath;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCreate))]
    [NotifyPropertyChangedFor(nameof(CanGoToParent))]
    [NotifyPropertyChangedFor(nameof(DirectoryListingSummary))]
    private bool _isLoadingDirectory;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCreate))]
    [NotifyPropertyChangedFor(nameof(CanGoToParent))]
    private bool _isSaving;

    [ObservableProperty]
    private bool _showsHiddenDirectories;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCreate))]
    private string _projectName = string.Empty;

    [ObservableProperty]
    private string _projectDescription = string.Empty;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCreate))]
    private ProjectTypeOption? _selectedProjectType;

    [ObservableProperty]
    private bool _isDirectoryWritable;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCreate))]
    [NotifyPropertyChangedFor(nameof(DirectoryListingSummary))]
    private bool _hasUsableDirectoryListing;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(DirectoryListingSummary))]
    private int _directoryEntryCount;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(DirectoryListingSummary))]
    private int _directoryFolderCount;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(DirectoryListingSummary))]
    private int _directoryFileCount;

    public bool CanCreate =>
        !_disposed && NormalizedProjectName.Length > 0 &&
        SelectedWorkspace is not null &&
        SelectedProjectType is not null &&
        CurrentPath.Length > 0 &&
        HasUsableDirectoryListing &&
        !IsLoadingDirectory &&
        !IsSaving;

    public bool CanGoToParent =>
        !_disposed &&
        !IsLoadingDirectory &&
        !IsSaving &&
        SelectedWorkspace is not null &&
        CurrentPath.Length > 0 &&
        ParentPath is not null &&
        !PathsEqual(CurrentPath, SelectedWorkspace.AbsoluteRoot);

    public string DirectoryListingSummary => IsLoadingDirectory
        ? L("正在验证目录内容…", "Verifying directory contents…")
        : !HasUsableDirectoryListing
            ? L("目录内容尚未验证", "Directory contents are not verified")
            : L(
                $"目录项 {DirectoryEntryCount}（文件夹 {DirectoryFolderCount}，文件 {DirectoryFileCount}）",
                $"{DirectoryEntryCount} entries ({DirectoryFolderCount} folders, {DirectoryFileCount} files)");

    public string DisplayedLocation => SelectedWorkspace is null
        ? L("没有可用工作区", "No workspace available")
        : CurrentRelativePath is { Length: > 0 } relative
            ? $"{SelectedWorkspace.Alias}/{relative}"
            : SelectedWorkspace.Alias;

    public async Task LoadInitialDirectoryAsync(CancellationToken cancellationToken = default)
    {
        if (CurrentPath.Length > 0) return;
        await OpenInitialDirectoryAsync(cancellationToken);
    }

    public async Task SelectWorkspaceAsync(
        LocalConnectorWorkspaceStatus? workspace,
        CancellationToken cancellationToken = default)
    {
        if (_disposed || IsSaving) return;
        if (workspace is not null && !Workspaces.Contains(workspace)) return;
        SelectedWorkspace = workspace;
        ResetDirectory();
        await OpenInitialDirectoryAsync(cancellationToken);
    }

    public Task OpenWorkspaceRootAsync(CancellationToken cancellationToken = default)
    {
        if (_disposed || IsSaving) return Task.CompletedTask;
        if (SelectedWorkspace is null)
        {
            ResetDirectory();
            return Task.CompletedTask;
        }

        return LoadDirectoryAsync(SelectedWorkspace.AbsoluteRoot, null, false, cancellationToken);
    }

    public Task OpenDirectoryAsync(
        ProjectFileEntry? entry,
        CancellationToken cancellationToken = default)
    {
        if (entry is not { IsDirectory: true }) return Task.CompletedTask;
        return LoadDirectoryAsync(
            entry.Path,
            RelativePathFor(entry.Path),
            false,
            cancellationToken);
    }

    public Task GoToParentDirectoryAsync(CancellationToken cancellationToken = default)
    {
        // The workspace root is the navigation boundary even if a connector
        // returns a stale/self parent value for a filesystem root.  macOS
        // treats a nil relative path as the root and disables "up" there;
        // keep Windows navigation consistent and avoid reloading the same
        // root forever.
        if (!CanGoToParent || ParentPath is null)
        {
            return Task.CompletedTask;
        }
        return LoadDirectoryAsync(
            ParentPath,
            RelativePathFor(ParentPath),
            false,
            cancellationToken);
    }

    public Task OpenAbsolutePathAsync(
        string rawPath,
        CancellationToken cancellationToken = default)
    {
        if (_disposed || IsSaving || IsLoadingDirectory || SelectedWorkspace is null)
        {
            return Task.CompletedTask;
        }

        var candidate = rawPath.Trim();
        try
        {
            if (!Path.IsPathFullyQualified(candidate))
            {
                throw new InvalidDataException("The directory path is not absolute.");
            }

            candidate = Path.GetFullPath(candidate);
            var relativePath = RelativePathWithinWorkspace(SelectedWorkspace, candidate);
            return LoadDirectoryAsync(candidate, relativePath, false, cancellationToken);
        }
        catch (Exception exception) when (exception is ArgumentException or IOException or InvalidDataException or NotSupportedException)
        {
            ErrorMessage = L(
                "请输入授权工作区内的有效绝对目录路径。",
                "Enter a valid absolute directory path inside the authorized workspace.");
            return Task.CompletedTask;
        }
    }

    public Task RefreshDirectoryAsync(CancellationToken cancellationToken = default)
    {
        if (CurrentPath.Length == 0) return Task.CompletedTask;
        return LoadDirectoryAsync(CurrentPath, CurrentRelativePath, true, cancellationToken);
    }

    public void ToggleHiddenDirectories()
    {
        if (_disposed || IsSaving) return;
        ShowsHiddenDirectories = !ShowsHiddenDirectories;
        ApplyDirectoryFilter();
    }

    public async Task CreateDirectoryAsync(
        string rawName,
        CancellationToken cancellationToken = default)
    {
        if (_disposed || IsSaving || IsLoadingDirectory) return;
        var name = rawName.Trim();
        if (!IsValidDirectoryName(name))
        {
            ErrorMessage = L(
                "文件夹名称不能为空，也不能包含 / 或 \\。",
                "The folder name cannot be empty or contain / or \\.");
            return;
        }

        if (CurrentPath.Length == 0 || !IsDirectoryWritable || SelectedWorkspace is null) return;
        var path = CurrentPath;
        var relativePath = CurrentRelativePath;
        var request = BeginDirectoryRequest(cancellationToken);
        try
        {
            await _filesystem.CreateDirectoryAsync(path, name, request.Token);
            if (!CanApplyDirectoryResult(request)) return;
            var listing = await _filesystem.ListEntriesAsync(path, true, request.Token);
            if (!CanApplyDirectoryResult(request)) return;
            ApplyVerifiedDirectoryListing(
                request,
                path,
                relativePath,
                listing,
                suggestName: false);
        }
        catch (OperationCanceledException) when (request.Token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            if (CanApplyDirectoryResult(request)) ErrorMessage = exception.Message;
        }
        finally
        {
            if (IsLatestDirectoryRequest(request)) IsLoadingDirectory = false;
        }
    }

    public void UpdateProjectName(string value)
    {
        ProjectName = value;
        _userEditedProjectName = true;
    }

    public async Task<bool> SaveAsync(
        Func<LocalProjectDraft, string, CancellationToken, Task> create,
        CancellationToken cancellationToken = default)
    {
        if (_disposed || IsSaving || IsLoadingDirectory) return false;
        if (SelectedWorkspace is null || CurrentPath.Length == 0)
        {
            ErrorMessage = L(
                "请选择可访问的本机项目目录。",
                "Select an accessible local project directory.");
            return false;
        }
        if (!HasUsableDirectoryListing)
        {
            ErrorMessage = L(
                "所选目录的内容尚未验证，请刷新目录后重试。",
                "The selected directory has not been verified. Refresh the directory and try again.");
            return false;
        }

        var draft = new LocalProjectDraft(
            NormalizedProjectName,
            SelectedWorkspace.Id,
            CurrentRelativePath ?? string.Empty,
            ProjectDescription.Trim(),
            SelectedProjectType?.Key ?? LocalProjectTypeCatalog.LegacyProjectTypeKey);
        var workspace = SelectedWorkspace;
        var selectedPath = CurrentPath;
        var selectedRelativePath = CurrentRelativePath;
        var request = BeginDirectoryRequest(cancellationToken);
        IsSaving = true;
        ErrorMessage = null;
        try
        {
            draft.Validate();
            ProjectDirectoryListing listing;
            try
            {
                listing = await _filesystem.ListEntriesAsync(
                    selectedPath,
                    forceRefresh: true,
                    request.Token);
                var actualRelativePath = VerifyDirectoryListingIdentity(
                    workspace,
                    selectedPath,
                    selectedRelativePath,
                    listing);
                if (!CanApplyDirectoryResult(request)) return false;
                ApplyDirectoryListing(listing, actualRelativePath, suggestName: false);
            }
            catch (OperationCanceledException) when (request.Token.IsCancellationRequested)
            {
                return false;
            }
            catch
            {
                InvalidateDirectoryListing();
                throw;
            }

            await create(draft, workspace.AbsoluteRoot, request.Token);
            return !_disposed;
        }
        catch (OperationCanceledException) when (request.Token.IsCancellationRequested)
        {
            return false;
        }
        catch (Exception exception)
        {
            if (!_disposed) ErrorMessage = exception.Message;
            return false;
        }
        finally
        {
            if (IsLatestDirectoryRequest(request)) IsLoadingDirectory = false;
            if (!_disposed) IsSaving = false;
        }
    }

    private string NormalizedProjectName => ProjectName.Trim();

    private async Task OpenInitialDirectoryAsync(CancellationToken cancellationToken)
    {
        if (SelectedWorkspace is null)
        {
            ResetDirectory();
            return;
        }

        var homeRelative = RelativeHomePath();
        if (homeRelative is null)
        {
            await LoadDirectoryAsync(
                SelectedWorkspace.AbsoluteRoot,
                null,
                false,
                cancellationToken);
            return;
        }

        await LoadDirectoryAsync(
            Path.Combine(SelectedWorkspace.AbsoluteRoot, homeRelative),
            NormalizeRelative(homeRelative),
            false,
            cancellationToken);
    }

    private async Task LoadDirectoryAsync(
        string path,
        string? relativePath,
        bool forceRefresh,
        CancellationToken cancellationToken)
    {
        if (_disposed || IsSaving || SelectedWorkspace is null) return;
        var request = BeginDirectoryRequest(cancellationToken);
        try
        {
            var listing = await _filesystem.ListEntriesAsync(path, forceRefresh, request.Token);
            if (!CanApplyDirectoryResult(request)) return;
            ApplyVerifiedDirectoryListing(
                request,
                path,
                relativePath,
                listing,
                suggestName: true);
        }
        catch (OperationCanceledException) when (request.Token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            if (!CanApplyDirectoryResult(request)) return;
            ErrorMessage = exception.Message;
            IsDirectoryWritable = false;
            HasUsableDirectoryListing = false;
            DirectoryEntryCount = 0;
            DirectoryFolderCount = 0;
            DirectoryFileCount = 0;
            _allDirectoryEntries.Clear();
            Entries.Clear();
        }
        finally
        {
            if (IsLatestDirectoryRequest(request)) IsLoadingDirectory = false;
        }
    }

    private void ApplyDirectoryListing(ProjectDirectoryListing listing, string? relativePath, bool suggestName)
    {
        var entries = listing.Entries.ToArray();
        CurrentPath = listing.Path;
        CurrentRelativePath = relativePath;
        ParentPath = listing.ParentPath;
        IsDirectoryWritable = listing.IsWritable;
        DirectoryEntryCount = entries.Length;
        DirectoryFolderCount = entries.Count(static entry => entry.IsDirectory);
        DirectoryFileCount = entries.Length - DirectoryFolderCount;
        HasUsableDirectoryListing = true;
        ReplaceAllDirectories(entries);
        if (suggestName) ApplySuggestedProjectName();
    }

    private void ApplyVerifiedDirectoryListing(
        DirectoryRequest request,
        string requestedPath,
        string? requestedRelativePath,
        ProjectDirectoryListing listing,
        bool suggestName)
    {
        var actualRelativePath = VerifyDirectoryListingIdentity(
            request.Workspace,
            requestedPath,
            requestedRelativePath,
            listing);

        ApplyDirectoryListing(listing, actualRelativePath, suggestName);
    }

    private string? VerifyDirectoryListingIdentity(
        LocalConnectorWorkspaceStatus workspace,
        string requestedPath,
        string? requestedRelativePath,
        ProjectDirectoryListing listing)
    {
        string? actualRelativePath;
        try
        {
            actualRelativePath = RelativePathWithinWorkspace(workspace, listing.Path);
        }
        catch (InvalidDataException exception)
        {
            throw DirectoryIdentityMismatch(exception);
        }
        if (!PathsEqual(requestedPath, listing.Path) ||
            !RelativePathsEqual(requestedRelativePath, actualRelativePath))
        {
            throw DirectoryIdentityMismatch();
        }

        return actualRelativePath;
    }

    private InvalidDataException DirectoryIdentityMismatch(Exception? innerException = null) =>
        new(
            L(
                "目录响应与所选位置不一致，请刷新目录后重试。",
                "The directory response does not match the selected location. Refresh the directory and try again."),
            innerException);

    private void InvalidateDirectoryListing()
    {
        HasUsableDirectoryListing = false;
        IsDirectoryWritable = false;
        DirectoryEntryCount = 0;
        DirectoryFolderCount = 0;
        DirectoryFileCount = 0;
        _allDirectoryEntries.Clear();
        Entries.Clear();
    }

    private static string? RelativePathWithinWorkspace(
        LocalConnectorWorkspaceStatus workspace,
        string path)
    {
        if (!Path.IsPathFullyQualified(path))
        {
            throw new InvalidDataException("The directory response path is not absolute.");
        }

        var root = Path.TrimEndingDirectorySeparator(Path.GetFullPath(workspace.AbsoluteRoot));
        var candidate = Path.TrimEndingDirectorySeparator(Path.GetFullPath(path));
        if (PathsEqual(root, candidate)) return null;
        if (!candidate.StartsWith(DirectoryPrefix(root), StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException("The directory response escaped the selected workspace.");
        }

        return NormalizeRelative(Path.GetRelativePath(root, candidate));
    }

    private static bool RelativePathsEqual(string? left, string? right) =>
        string.Equals(left ?? string.Empty, right ?? string.Empty, StringComparison.OrdinalIgnoreCase);

    private sealed record DirectoryRequest(
        long Generation, LocalConnectorWorkspaceStatus Workspace, CancellationToken Token);

    private DirectoryRequest BeginDirectoryRequest(CancellationToken cancellationToken)
    {
        CancelDirectoryRequest();
        _directoryCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var request = new DirectoryRequest(_directoryGeneration, SelectedWorkspace!, _directoryCancellation.Token);
        ErrorMessage = null;
        IsLoadingDirectory = true;
        return request;
    }

    private bool IsLatestDirectoryRequest(DirectoryRequest request) =>
        !_disposed && request.Generation == _directoryGeneration &&
        ReferenceEquals(request.Workspace, SelectedWorkspace);

    private bool CanApplyDirectoryResult(DirectoryRequest request) =>
        IsLatestDirectoryRequest(request) && !request.Token.IsCancellationRequested;

    private void CancelDirectoryRequest()
    {
        _directoryGeneration++;
        _directoryCancellation?.Cancel();
        _directoryCancellation?.Dispose();
        _directoryCancellation = null;
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        CancelDirectoryRequest();
        IsLoadingDirectory = false;
        IsSaving = false;
        OnPropertyChanged(nameof(CanCreate));
    }

    private void ReplaceAllDirectories(IEnumerable<ProjectFileEntry> entries)
    {
        _allDirectoryEntries.Clear();
        _allDirectoryEntries.AddRange(entries
            .Where(static entry => entry.IsDirectory)
            .OrderBy(static entry => entry.Name, StringComparer.CurrentCultureIgnoreCase));
        ApplyDirectoryFilter();
    }

    private void ApplyDirectoryFilter()
    {
        Entries.Clear();
        foreach (var entry in _allDirectoryEntries)
        {
            if (!ShowsHiddenDirectories && ShouldHide(entry.Name)) continue;
            Entries.Add(entry);
        }
    }

    private bool ShouldHide(string name)
    {
        if (name.StartsWith(".", StringComparison.Ordinal)) return true;
        if (IsWindowsSystemRoot && WindowsRootSystemDirectories.Contains(name)) return true;
        return IsWindowsUserHome && WindowsUserSystemDirectories.Contains(name);
    }

    private bool IsWindowsSystemRoot =>
        SelectedWorkspace is not null &&
        CurrentRelativePath is null &&
        Path.GetPathRoot(SelectedWorkspace.AbsoluteRoot) is { } root &&
        PathsEqual(root, SelectedWorkspace.AbsoluteRoot);

    private bool IsWindowsUserHome =>
        string.Equals(CurrentRelativePath, RelativeHomePath(), StringComparison.OrdinalIgnoreCase);

    private void ApplySuggestedProjectName()
    {
        if (_userEditedProjectName) return;
        ProjectName = CurrentRelativePath is { Length: > 0 }
            ? CurrentRelativePath.Split('/').Last()
            : SelectedWorkspace?.Alias ?? string.Empty;
    }

    private string? RelativePathFor(string logicalPath)
    {
        if (SelectedWorkspace is null) return CurrentRelativePath;
        try
        {
            var root = Path.TrimEndingDirectorySeparator(Path.GetFullPath(SelectedWorkspace.AbsoluteRoot));
            var candidate = Path.GetFullPath(logicalPath);
            if (PathsEqual(root, candidate)) return null;
            var prefix = DirectoryPrefix(root);
            if (!candidate.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)) return CurrentRelativePath;
            return NormalizeRelative(Path.GetRelativePath(root, candidate));
        }
        catch (Exception) when (logicalPath.Contains("://", StringComparison.Ordinal))
        {
            return CurrentRelativePath;
        }
    }

    private string? RelativeHomePath()
    {
        if (SelectedWorkspace is null) return null;
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        if (home.Length == 0) return null;
        try
        {
            var root = Path.TrimEndingDirectorySeparator(Path.GetFullPath(SelectedWorkspace.AbsoluteRoot));
            var fullHome = Path.TrimEndingDirectorySeparator(Path.GetFullPath(home));
            if (PathsEqual(root, fullHome)) return null;
            var prefix = DirectoryPrefix(root);
            return fullHome.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)
                ? NormalizeRelative(Path.GetRelativePath(root, fullHome))
                : null;
        }
        catch
        {
            return null;
        }
    }

    private void ResetDirectory()
    {
        CancelDirectoryRequest();
        IsLoadingDirectory = false;
        CurrentPath = string.Empty;
        CurrentRelativePath = null;
        ParentPath = null;
        IsDirectoryWritable = false;
        HasUsableDirectoryListing = false;
        DirectoryEntryCount = 0;
        DirectoryFolderCount = 0;
        DirectoryFileCount = 0;
        _allDirectoryEntries.Clear();
        Entries.Clear();
        ErrorMessage = null;
    }

    private static bool IsValidDirectoryName(string name) =>
        name.Length > 0 &&
        name is not "." and not ".." &&
        name.IndexOfAny(['/', '\\', '\0']) < 0;

    private static bool PathsEqual(string left, string right) =>
        string.Equals(
            Path.TrimEndingDirectorySeparator(Path.GetFullPath(left)),
            Path.TrimEndingDirectorySeparator(Path.GetFullPath(right)),
            StringComparison.OrdinalIgnoreCase);

    private static string DirectoryPrefix(string path) =>
        Path.EndsInDirectorySeparator(path)
            ? path
            : path + Path.DirectorySeparatorChar;

    private static string NormalizeRelative(string path) =>
        path.Replace(Path.DirectorySeparatorChar, '/').Replace(Path.AltDirectorySeparatorChar, '/').Trim('/');

    private string L(string chinese, string english) =>
        _localization?.Text(chinese, english) ?? chinese;
}

public sealed record ProjectTypeOption(
    string Key,
    string Label,
    string Description,
    string CategoryLabel)
{
    public string DisplayLabel => $"{CategoryLabel} · {Label}";
}
