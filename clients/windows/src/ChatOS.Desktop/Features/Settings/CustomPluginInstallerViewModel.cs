using System.Text;
using System.Text.Json;
using ChatOS.Connector.Plugins;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.Settings;

public sealed partial class CustomPluginInstallerViewModel : ObservableObject
{
    private readonly ILocalPluginSourceInstaller _installer;
    private readonly LocalizationViewModel _localization;
    private readonly IUiDispatcher _dispatcher;
    private readonly SemaphoreSlim _operationGate = new(1, 1);
    private readonly object _operationCancellationGate = new();
    private CancellationTokenSource? _operationCancellation;

    public CustomPluginInstallerViewModel(
        ILocalPluginSourceInstaller installer,
        LocalizationViewModel localization,
        IUiDispatcher dispatcher)
    {
        _installer = installer;
        _localization = localization;
        _dispatcher = dispatcher;
        _localization.PropertyChanged += (_, _) => NotifyLocalizedProperties();
    }

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanInspect))]
    private string _sourcePath = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsLocalSource))]
    [NotifyPropertyChangedFor(nameof(IsNpmSource))]
    [NotifyPropertyChangedFor(nameof(IsGitSource))]
    [NotifyPropertyChangedFor(nameof(CanInspect))]
    private bool _useNpmPackage;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsLocalSource))]
    [NotifyPropertyChangedFor(nameof(IsNpmSource))]
    [NotifyPropertyChangedFor(nameof(IsGitSource))]
    [NotifyPropertyChangedFor(nameof(CanInspect))]
    private bool _useGitRepository;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanInspect))]
    private string _gitRepositoryUrl = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanInspect))]
    private string _gitReference = string.Empty;

    [ObservableProperty]
    private string _gitSubdirectory = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanInspect))]
    private string _npmPackageName = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanInspect))]
    private string _npmPackageVersion = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanInspect))]
    private string _npmBinaryName = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanInspect))]
    private string _npmComponentKey = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanInspect))]
    private string _npmDisplayName = string.Empty;

    [ObservableProperty]
    private string _npmDescription = string.Empty;

    [ObservableProperty]
    private string _npmArgumentsJson = "[]";

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasInspection))]
    [NotifyPropertyChangedFor(nameof(CanInstall))]
    [NotifyPropertyChangedFor(nameof(InspectionTitle))]
    [NotifyPropertyChangedFor(nameof(InspectionSource))]
    [NotifyPropertyChangedFor(nameof(InspectionComponents))]
    [NotifyPropertyChangedFor(nameof(InspectionFingerprint))]
    private LocalPluginInspection? _inspection;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanInspect))]
    [NotifyPropertyChangedFor(nameof(CanInstall))]
    [NotifyPropertyChangedFor(nameof(BusyLabel))]
    private bool _isBusy;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _actionMessage;

    public bool HasInspection => Inspection is not null;

    public bool IsLocalSource => !UseNpmPackage && !UseGitRepository;

    public bool IsNpmSource => UseNpmPackage && !UseGitRepository;

    public bool IsGitSource => UseGitRepository && !UseNpmPackage;

    public bool CanInspect => !IsBusy && (IsNpmSource
        ? !string.IsNullOrWhiteSpace(NpmPackageName) &&
          !string.IsNullOrWhiteSpace(NpmPackageVersion) &&
          !string.IsNullOrWhiteSpace(NpmBinaryName) &&
          !string.IsNullOrWhiteSpace(NpmComponentKey) &&
          !string.IsNullOrWhiteSpace(NpmDisplayName)
        : IsGitSource
            ? !string.IsNullOrWhiteSpace(GitRepositoryUrl) &&
              !string.IsNullOrWhiteSpace(GitReference)
            : !string.IsNullOrWhiteSpace(SourcePath));

    public bool CanInstall => !IsBusy && Inspection is not null;

    public string SectionTitle => _localization.Text(
        "导入 Plugin / Skill / MCP",
        "Import Plugin / Skill / MCP");

    public string SectionDescription => _localization.Text(
        "选择本地根目录、无凭据 HTTPS Git 仓库与明确 ref，或固定版本 npm stdio MCP。ChatOS 只在隔离副本中获取、归一化和校验；MCP 会真实初始化并读取工具清单。",
        "Choose a local root, a credential-free HTTPS Git repository with an explicit ref, or a pinned-version npm stdio MCP. ChatOS acquires, normalizes and validates only an isolated copy, then actually initializes MCP servers and queries their tools.");

    public string NpmModeLabel => _localization.Text(
        "使用固定版本 npm MCP",
        "Use pinned npm MCP");

    public string GitModeLabel => _localization.Text(
        "使用 HTTPS Git 来源",
        "Use HTTPS Git source");

    public string GitRepositoryHeader => _localization.Text("HTTPS 仓库 URL", "HTTPS repository URL");

    public string GitReferenceHeader => _localization.Text("明确 ref（分支、标签或提交）", "Explicit ref (branch, tag, or commit)");

    public string GitSubdirectoryHeader => _localization.Text("仓库子目录（可选）", "Repository subdirectory (optional)");

    public string NpmPackageNameHeader => _localization.Text("npm 包名", "npm package");

    public string NpmVersionHeader => _localization.Text("精确版本", "Exact version");

    public string NpmBinaryHeader => _localization.Text("可执行名称", "Binary name");

    public string NpmComponentHeader => _localization.Text("组件键", "Component key");

    public string NpmDisplayNameHeader => _localization.Text("显示名称", "Display name");

    public string NpmDescriptionHeader => _localization.Text("说明", "Description");

    public string NpmArgumentsHeader => _localization.Text(
        "参数（JSON 字符串数组）",
        "Arguments (JSON string array)");

    public string SourceHeader => _localization.Text(
        "Plugin / Skill / MCP 根目录",
        "Plugin / Skill / MCP root folder");

    public string SourcePlaceholder => _localization.Text(
        @"例如：C:\Downloads\my-plugin",
        @"Example: C:\Downloads\my-plugin");

    public string ChooseFolderLabel => _localization.Text("选择根目录…", "Choose root folder…");

    public string InspectLabel => _localization.Text("预检工具", "Inspect tools");

    public string InstallLabel => _localization.Text("确认安装", "Confirm install");

    public string BusyLabel => IsBusy
        ? _localization.Text("正在隔离预检…", "Inspecting in isolation…")
        : string.Empty;

    public string SafetyLabel => _localization.Text(
        "接受本地 stdio MCP、可校验 Skill、无凭据 HTTPS Git 与固定语义版本 npm stdio MCP；Git 禁 hooks/file protocol 和交互凭据，npm 禁 lifecycle scripts。HTTP/SSE、外部 UI 与明文环境变量仍会明确拒绝。安装前会重新获取并核对指纹，任何来源、Skill 或工具清单变化都会拒绝安装。",
        "Local stdio MCP servers, verifiable Skills, credential-free HTTPS Git sources and exact-semver npm stdio MCP packages are accepted. Git hooks, the file protocol and interactive credentials are disabled; npm lifecycle scripts are disabled. HTTP/SSE, external UI and plaintext environment values are rejected. ChatOS reacquires and fingerprints the source before installation and rejects any source, Skill or tool-list drift.");

    public string InspectionPassedLabel => _localization.Text("预检通过", "Preflight passed");

    public string InspectionTitle => Inspection is null
        ? string.Empty
        : $"{Inspection.DisplayName} · {Inspection.Version}";

    public string InspectionSource => Inspection?.SourceSummary ?? string.Empty;

    public string InspectionFingerprint => Inspection is null
        ? string.Empty
        : $"SHA-256 {Inspection.Fingerprint}";

    public string InspectionComponents
    {
        get
        {
            if (Inspection is null) return string.Empty;
            var builder = new StringBuilder();
            foreach (var component in Inspection.Components)
            {
                if (builder.Length > 0) builder.AppendLine();
                builder.Append(component.DisplayName);
                if (component.Kind is LocalPluginInspectionComponentKind.Skill)
                {
                    builder.Append(" · Skill");
                    continue;
                }

                builder.Append(_localization.Text(
                    $" · {component.ToolNames.Count} 个工具",
                    $" · {component.ToolNames.Count} tools"));
                if (component.ToolNames.Count > 0)
                {
                    builder.AppendLine();
                    builder.Append(string.Join(", ", component.ToolNames.Take(24)));
                    if (component.ToolNames.Count > 24)
                    {
                        builder.Append($" +{component.ToolNames.Count - 24}");
                    }
                }
            }
            return builder.ToString();
        }
    }

    partial void OnSourcePathChanged(string value)
    {
        InvalidateInspection();
    }

    partial void OnUseNpmPackageChanged(bool value)
    {
        if (value && UseGitRepository) UseGitRepository = false;
        InvalidateInspection();
    }

    partial void OnUseGitRepositoryChanged(bool value)
    {
        if (value && UseNpmPackage) UseNpmPackage = false;
        InvalidateInspection();
    }

    partial void OnGitRepositoryUrlChanged(string value) => InvalidateInspection();

    partial void OnGitReferenceChanged(string value) => InvalidateInspection();

    partial void OnGitSubdirectoryChanged(string value) => InvalidateInspection();

    partial void OnNpmPackageNameChanged(string value) => InvalidateInspection();

    partial void OnNpmPackageVersionChanged(string value) => InvalidateInspection();

    partial void OnNpmBinaryNameChanged(string value) => InvalidateInspection();

    partial void OnNpmComponentKeyChanged(string value) => InvalidateInspection();

    partial void OnNpmDisplayNameChanged(string value) => InvalidateInspection();

    partial void OnNpmDescriptionChanged(string value) => InvalidateInspection();

    partial void OnNpmArgumentsJsonChanged(string value) => InvalidateInspection();

    private void InvalidateInspection()
    {
        CancelPending();
        Inspection = null;
        ErrorMessage = null;
        ActionMessage = null;
    }

    public Task InspectAsync(CancellationToken cancellationToken = default) =>
        RunAsync(async token =>
        {
            var inspection = IsNpmSource
                ? await _installer.InspectNpmMcpAsync(CaptureNpmSource(), token).ConfigureAwait(false)
                : await _installer.InspectAsync(CapturePluginSource(), token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => Inspection = inspection, token).ConfigureAwait(false);
        }, cancellationToken, clearInspection: true);

    public void CancelPending()
    {
        lock (_operationCancellationGate)
        {
            _operationCancellation?.Cancel();
        }
    }

    public async Task<bool> InstallAsync(CancellationToken cancellationToken = default)
    {
        var installed = false;
        await RunAsync(async token =>
        {
            var expected = Inspection?.Fingerprint
                ?? throw new InvalidOperationException(_localization.Text(
                    "请先完成预检。",
                    "Complete preflight first."));
            var record = IsNpmSource
                ? await _installer.InstallNpmMcpAsync(CaptureNpmSource(), expected, token)
                    .ConfigureAwait(false)
                : await _installer.InstallAsync(CapturePluginSource(), expected, token)
                    .ConfigureAwait(false);
            installed = true;
            await _dispatcher.InvokeAsync(() =>
            {
                ActionMessage = _localization.Text(
                    $"{record.PluginKey ?? record.PluginId} 已安装并通过来源指纹校验。",
                    $"{record.PluginKey ?? record.PluginId} was installed and passed source fingerprint validation.");
                Inspection = null;
            }, token).ConfigureAwait(false);
        }, cancellationToken).ConfigureAwait(false);
        return installed;
    }

    private LocalPluginSourceDraft CapturePluginSource()
    {
        if (IsGitSource)
        {
            var url = GitRepositoryUrl.Trim();
            var reference = GitReference.Trim();
            if (url.Length == 0 || reference.Length == 0)
            {
                throw new InvalidOperationException(_localization.Text(
                    "请填写 HTTPS Git 仓库和明确 ref。",
                    "Enter an HTTPS Git repository and an explicit ref."));
            }
            return new LocalPluginSourceDraft(
                LocalPluginSourceKind.GitRepository,
                url,
                GitReference: reference,
                Subdirectory: string.IsNullOrWhiteSpace(GitSubdirectory)
                    ? null
                    : GitSubdirectory.Trim());
        }
        var path = SourcePath.Trim();
        if (path.Length == 0)
        {
            throw new InvalidOperationException(_localization.Text(
                "请选择 Plugin / MCP 根目录。",
                "Choose a Plugin / MCP root folder."));
        }
        return new LocalPluginSourceDraft(LocalPluginSourceKind.LocalFolder, path);
    }

    private NpmMcpDraft CaptureNpmSource()
    {
        string[] arguments;
        try
        {
            arguments = JsonSerializer.Deserialize<string[]>(NpmArgumentsJson.Trim())
                ?? throw new JsonException();
        }
        catch (JsonException exception)
        {
            throw new InvalidOperationException(_localization.Text(
                "npm 参数必须是 JSON 字符串数组，例如 [\"mcp\"]。",
                "npm arguments must be a JSON string array, for example [\"mcp\"]."), exception);
        }
        if (arguments.Any(static value => value is null))
        {
            throw new InvalidOperationException(_localization.Text(
                "npm 参数不能包含 null。",
                "npm arguments cannot contain null."));
        }
        return new NpmMcpDraft(
            NpmDisplayName.Trim(),
            NpmDescription.Trim(),
            NpmComponentKey.Trim(),
            NpmPackageName.Trim(),
            NpmPackageVersion.Trim(),
            NpmBinaryName.Trim(),
            arguments);
    }

    private async Task RunAsync(
        Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken,
        bool clearInspection = false)
    {
        if (!await _operationGate.WaitAsync(0, cancellationToken).ConfigureAwait(false)) return;
        using var linkedCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        lock (_operationCancellationGate)
        {
            _operationCancellation = linkedCancellation;
        }
        try
        {
            await _dispatcher.InvokeAsync(() =>
            {
                IsBusy = true;
                ErrorMessage = null;
                ActionMessage = null;
                if (clearInspection) Inspection = null;
            }, linkedCancellation.Token).ConfigureAwait(false);
            await operation(linkedCancellation.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (linkedCancellation.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message, CancellationToken.None)
                .ConfigureAwait(false);
        }
        finally
        {
            lock (_operationCancellationGate)
            {
                if (ReferenceEquals(_operationCancellation, linkedCancellation))
                {
                    _operationCancellation = null;
                }
            }
            await _dispatcher.InvokeAsync(() => IsBusy = false, CancellationToken.None)
                .ConfigureAwait(false);
            _operationGate.Release();
        }
    }

    private void NotifyLocalizedProperties()
    {
        OnPropertyChanged(nameof(SectionTitle));
        OnPropertyChanged(nameof(SectionDescription));
        OnPropertyChanged(nameof(NpmModeLabel));
        OnPropertyChanged(nameof(GitModeLabel));
        OnPropertyChanged(nameof(GitRepositoryHeader));
        OnPropertyChanged(nameof(GitReferenceHeader));
        OnPropertyChanged(nameof(GitSubdirectoryHeader));
        OnPropertyChanged(nameof(NpmPackageNameHeader));
        OnPropertyChanged(nameof(NpmVersionHeader));
        OnPropertyChanged(nameof(NpmBinaryHeader));
        OnPropertyChanged(nameof(NpmComponentHeader));
        OnPropertyChanged(nameof(NpmDisplayNameHeader));
        OnPropertyChanged(nameof(NpmDescriptionHeader));
        OnPropertyChanged(nameof(NpmArgumentsHeader));
        OnPropertyChanged(nameof(SourceHeader));
        OnPropertyChanged(nameof(SourcePlaceholder));
        OnPropertyChanged(nameof(ChooseFolderLabel));
        OnPropertyChanged(nameof(InspectLabel));
        OnPropertyChanged(nameof(InstallLabel));
        OnPropertyChanged(nameof(BusyLabel));
        OnPropertyChanged(nameof(SafetyLabel));
        OnPropertyChanged(nameof(InspectionPassedLabel));
        OnPropertyChanged(nameof(InspectionComponents));
    }
}
