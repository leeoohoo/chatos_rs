using System.Collections.ObjectModel;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Api.Tasks;
using ChatOS.Connector.Plugins;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Desktop.AppShell;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Microsoft.Web.WebView2.Core;
using Windows.UI;

namespace ChatOS.Desktop.Features.Plugins;

public sealed partial class PluginApplicationsPage : Page
{
    private const int MaximumBridgeMessageBytes = 256 * 1024;
    private static readonly HashSet<string> HostCapabilities =
    [
        "host.context.read",
        "task.batch.prepare",
        "task.batch.status",
        "task.workspace.open",
    ];
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = false,
        UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow,
    };
    private const string BootstrapScript = """
        (() => {
          if (window.top !== window) return;
          const pending = new Map();
          let ready = null;
          const receive = message => {
            if (!message || typeof message !== 'object') return;
            if (message.type === 'chatos.plugin_ui.ready') {
              ready = message;
              window.dispatchEvent(new CustomEvent('chatos:host-ready', { detail: message }));
              return;
            }
            if (message.type !== 'chatos.plugin_ui.response') return;
            const callback = pending.get(message.request_id);
            if (!callback) return;
            pending.delete(message.request_id);
            if (message.ok) callback.resolve(message.result);
            else callback.reject(Object.assign(new Error(message.error_message || 'Host request failed'), { code: message.error_code }));
          };
          window.chrome.webview.addEventListener('message', event => receive(event.data));
          window.chatosHost = Object.freeze({
            capabilities: () => ready ? [...ready.capabilities] : [],
            request: (method, payload = {}) => new Promise((resolve, reject) => {
              if (!ready) return reject(new Error('ChatOS host bridge is not ready'));
              if (!ready.capabilities.includes(method)) return reject(new Error(`Host capability is not granted: ${method}`));
              const request_id = crypto.randomUUID();
              pending.set(request_id, { resolve, reject });
              window.chrome.webview.postMessage(JSON.stringify({
                type: 'chatos.plugin_ui.request', protocol_version: 1,
                adapter_session_id: ready.adapter_session_id,
                host_session_nonce: ready.host_session_nonce,
                request_id, method, payload
              }));
            })
          });
        })();
        """;

    private readonly ILocalPluginApplicationService _applications;
    private readonly ILocalProjectsService _projects;
    private readonly IUserModelDefaultsService _defaults;
    private readonly IConversationRuntimeSettingsService _models;
    private readonly TaskRunnerHostService _taskRunner;
    private CancellationTokenSource? _launchCancellation;
    private LocalPluginApplication? _selectedApplication;
    private ShellResourceViewModel? _selectedProject;
    private ProjectContextSnapshot? _projectContext;
    private LocalPluginApplicationLaunch? _launch;
    private Uri? _allowedUrl;
    private bool _webViewInitialized;
    private string _adapterSessionId = string.Empty;
    private string _hostSessionNonce = string.Empty;
    private TaskWorkspaceState? _taskWorkspace;

    public PluginApplicationsPage(
        MainWindowViewModel shell,
        ILocalPluginApplicationService applications,
        ILocalProjectsService projects,
        IUserModelDefaultsService defaults,
        IConversationRuntimeSettingsService models,
        TaskRunnerHostService taskRunner,
        LocalizationViewModel localization)
    {
        Shell = shell;
        _applications = applications;
        _projects = projects;
        _defaults = defaults;
        _models = models;
        _taskRunner = taskRunner;
        Localization = localization;
        InitializeComponent();
    }

    public MainWindowViewModel Shell { get; }
    public LocalizationViewModel Localization { get; }
    public ObservableCollection<PluginApplicationCardViewModel> Applications { get; } = [];

    public string CatalogDescription => Localization.Text(
        "打开已安装并启用的插件应用。项目身份和目录始终由客户端提供。",
        "Open installed and enabled plugin applications. Project identity and paths always come from the client.");
    public string EmptyApplicationsTitle => Localization.Text("还没有插件应用", "No plugin applications");
    public string EmptyApplicationsDescription => Localization.Text(
        "请先在插件管理中安装并启用一个带工作台页面的插件。",
        "Install and enable a plugin with a workbench page in Plugin Management first.");
    public string ChooseProjectTitle => Localization.Text("选择应用项目", "Choose an application project");
    public string ChooseProjectDescription => Localization.Text(
        "插件会为当前 ChatOS 用户和项目使用独立数据目录；Git 和项目目录仍由客户端管理。",
        "The plugin uses isolated data for this ChatOS user and project; Git and the project directory remain client-owned.");
    public string NoProjectsDescription => Localization.Text(
        "当前没有可用项目。请先从侧栏创建本地项目。",
        "No project is available. Create a local project from the sidebar first.");
    public string StartingApplicationText => Localization.Text("正在启动插件应用…", "Starting plugin application…");
    public string ApplicationFailedTitle => Localization.Text("应用无法打开", "Application could not open");
    public string ApplicationHostName => Localization.Text("插件应用工作台", "Plugin application workbench");
    public string RetryText => Localization.Text("重试", "Try again");
    public string SwitchProjectText => Localization.Text("切换项目", "Switch project");
    public string TaskWorkspaceTitle => Localization.Text("任务工作区", "Task workspace");
    public string TaskWorkspaceDescription => Localization.Text(
        "Task Runner 是任务与运行状态的唯一权威来源",
        "Task Runner is the sole authority for task and run status");
    public string StartExecutionText => Localization.Text("开始执行", "Start execution");
    public string CloseText => Localization.Text("关闭", "Close");

    public async Task OpenAsync(CancellationToken cancellationToken = default)
    {
        ShowCatalog();
        await LoadApplicationsAsync(cancellationToken);
    }

    public async Task ResetAsync()
    {
        _launchCancellation?.Cancel();
        _launchCancellation?.Dispose();
        _launchCancellation = null;
        _selectedApplication = null;
        _selectedProject = null;
        _projectContext = null;
        _launch = null;
        _allowedUrl = null;
        _taskWorkspace = null;
        TaskWorkspaceOverlay.Visibility = Visibility.Collapsed;
        if (_webViewInitialized)
        {
            PluginWebView.CoreWebView2.Navigate("about:blank");
        }
        await _applications.StopAllAsync();
        ShowCatalog();
    }

    private async Task LoadApplicationsAsync(CancellationToken cancellationToken = default)
    {
        CatalogProgress.IsActive = true;
        CatalogMessageCard.Visibility = Visibility.Collapsed;
        try
        {
            var values = await _applications.ListAsync(cancellationToken);
            Applications.Clear();
            foreach (var value in values)
            {
                Applications.Add(new PluginApplicationCardViewModel(value, Localization));
            }
            EmptyApplications.Visibility = Applications.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
            ApplicationGrid.Visibility = Applications.Count == 0 ? Visibility.Collapsed : Visibility.Visible;
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            Applications.Clear();
            ApplicationGrid.Visibility = Visibility.Collapsed;
            EmptyApplications.Visibility = Visibility.Collapsed;
            CatalogMessageText.Text = exception.Message;
            CatalogMessageCard.Visibility = Visibility.Visible;
        }
        finally
        {
            CatalogProgress.IsActive = false;
        }
    }

    private async void OnRefreshApplicationsClicked(object sender, RoutedEventArgs e) =>
        await LoadApplicationsAsync();

    private async void OnApplicationClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is not PluginApplicationCardViewModel card) return;
        _selectedApplication = card.Application;
        _selectedProject = null;
        _projectContext = null;
        HostTitle.Text = card.Application.DisplayName;
        CatalogView.Visibility = Visibility.Collapsed;
        HostView.Visibility = Visibility.Visible;
        if (RequiresProject(card.Application))
        {
            ShowProjectPicker();
            return;
        }
        await LaunchAsync(null);
    }

    private void ShowProjectPicker()
    {
        ContextPicker.Visibility = Visibility.Visible;
        NoProjectsMessage.Visibility = Shell.Projects.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        ProjectList.Visibility = Shell.Projects.Count == 0 ? Visibility.Collapsed : Visibility.Visible;
        LaunchProgress.Visibility = Visibility.Collapsed;
        LaunchError.Visibility = Visibility.Collapsed;
        PluginWebView.Visibility = Visibility.Collapsed;
        ReloadApplicationButton.Visibility = Visibility.Collapsed;
        SwitchProjectButton.Visibility = Visibility.Collapsed;
        ProjectBadge.Visibility = Visibility.Collapsed;
    }

    private async void OnProjectClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is not ShellResourceViewModel { Kind: WorkspaceResourceKind.Project } project) return;
        _selectedProject = project;
        await LaunchAsync(project);
    }

    private async Task LaunchAsync(ShellResourceViewModel? project)
    {
        if (_selectedApplication is null) return;
        _launchCancellation?.Cancel();
        _launchCancellation?.Dispose();
        _launchCancellation = new CancellationTokenSource();
        var cancellationToken = _launchCancellation.Token;
        var accountGeneration = Shell.AccountGeneration;
        ContextPicker.Visibility = Visibility.Collapsed;
        LaunchError.Visibility = Visibility.Collapsed;
        PluginWebView.Visibility = Visibility.Collapsed;
        LaunchProgress.Visibility = Visibility.Visible;
        try
        {
            var owner = RequireOwner(accountGeneration);
            var context = project is null
                ? null
                : await _projects.ResolveContextAsync(owner, project.Id, cancellationToken);
            RequireOwner(accountGeneration, owner);
            var launch = await _applications.LaunchAsync(
                _selectedApplication.PluginId,
                _selectedApplication.ComponentKey,
                owner,
                context,
                cancellationToken);
            RequireOwner(accountGeneration, owner);
            _projectContext = context;
            _launch = launch;
            _allowedUrl = launch.Url;
            _adapterSessionId = Guid.NewGuid().ToString("D").ToLowerInvariant();
            _hostSessionNonce = $"{Guid.NewGuid():N}{Guid.NewGuid():N}";
            await InitializeWebViewAsync();
            PluginWebView.CoreWebView2.Navigate(launch.Url.AbsoluteUri);
            ProjectBadgeText.Text = context?.ProjectName ?? string.Empty;
            ProjectBadge.Visibility = context is null ? Visibility.Collapsed : Visibility.Visible;
            SwitchProjectButton.Visibility = RequiresProject(_selectedApplication)
                ? Visibility.Visible
                : Visibility.Collapsed;
            ReloadApplicationButton.Visibility = Visibility.Visible;
            LaunchProgress.Visibility = Visibility.Collapsed;
            PluginWebView.Visibility = Visibility.Visible;
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            LaunchProgress.Visibility = Visibility.Collapsed;
            LaunchErrorText.Text = exception.Message;
            LaunchError.Visibility = Visibility.Visible;
        }
        catch (OperationCanceledException)
        {
            LaunchProgress.Visibility = Visibility.Collapsed;
        }
    }

    private async Task InitializeWebViewAsync()
    {
        if (_webViewInitialized) return;
        await PluginWebView.EnsureCoreWebView2Async();
        var settings = PluginWebView.CoreWebView2.Settings;
        settings.AreDevToolsEnabled = false;
        settings.AreDefaultContextMenusEnabled = false;
        settings.IsStatusBarEnabled = false;
        settings.IsZoomControlEnabled = false;
        settings.IsWebMessageEnabled = true;
        await PluginWebView.CoreWebView2.AddScriptToExecuteOnDocumentCreatedAsync(BootstrapScript);
        PluginWebView.CoreWebView2.NavigationStarting += OnWebViewNavigationStarting;
        PluginWebView.CoreWebView2.NavigationCompleted += OnWebViewNavigationCompleted;
        PluginWebView.CoreWebView2.WebMessageReceived += OnWebMessageReceived;
        PluginWebView.CoreWebView2.NewWindowRequested += (_, args) => args.Handled = true;
        _webViewInitialized = true;
    }

    private void OnWebViewNavigationStarting(object? sender, CoreWebView2NavigationStartingEventArgs args)
    {
        if (!Uri.TryCreate(args.Uri, UriKind.Absolute, out var destination) || !Allows(destination))
        {
            args.Cancel = true;
        }
    }

    private void OnWebViewNavigationCompleted(object? sender, CoreWebView2NavigationCompletedEventArgs args)
    {
        if (!args.IsSuccess || _launch is null) return;
        SendBridgeMessage(new
        {
            type = "chatos.plugin_ui.ready",
            protocol_version = 1,
            adapter_session_id = _adapterSessionId,
            host_session_nonce = _hostSessionNonce,
            capabilities = GrantedCapabilities(),
        });
    }

    private async void OnWebMessageReceived(object? sender, CoreWebView2WebMessageReceivedEventArgs args)
    {
        string? requestId = null;
        try
        {
            if (_launch is null || !Uri.TryCreate(args.Source, UriKind.Absolute, out var source) || !Allows(source)) return;
            var raw = args.TryGetWebMessageAsString();
            if (Encoding.UTF8.GetByteCount(raw) > MaximumBridgeMessageBytes) return;
            using var document = JsonDocument.Parse(raw, new JsonDocumentOptions { MaxDepth = 32 });
            var root = document.RootElement;
            if (root.ValueKind != JsonValueKind.Object ||
                ReadString(root, "type") != "chatos.plugin_ui.request" ||
                !root.TryGetProperty("protocol_version", out var version) || version.GetInt32() != 1 ||
                ReadString(root, "adapter_session_id") != _adapterSessionId ||
                ReadString(root, "host_session_nonce") != _hostSessionNonce)
            {
                return;
            }
            requestId = ValidIdentifier(ReadString(root, "request_id"));
            var method = ReadString(root, "method");
            if (requestId is null || method is null || !GrantedCapabilities().Contains(method, StringComparer.Ordinal) ||
                !root.TryGetProperty("payload", out var payload) || payload.ValueKind != JsonValueKind.Object)
            {
                return;
            }
            var result = await HandleBridgeRequestAsync(method, payload.Clone());
            SendBridgeResponse(requestId, true, result);
        }
        catch (Exception exception)
        {
            if (requestId is not null)
            {
                SendBridgeResponse(
                    requestId,
                    false,
                    new { },
                    exception is OperationCanceledException ? "host_session_changed" : "host_request_failed",
                    exception.Message);
            }
        }
    }

    private async Task<object> HandleBridgeRequestAsync(string method, JsonElement payload)
    {
        return method switch
        {
            "host.context.read" => await ReadHostContextAsync(),
            "task.batch.prepare" => await PrepareTaskBatchAsync(payload),
            "task.batch.status" => await ReadTaskStatusesAsync(payload),
            "task.workspace.open" => await OpenTaskWorkspaceAsync(payload),
            _ => throw new InvalidOperationException("Host capability is not implemented."),
        };
    }

    private async Task<object> ReadHostContextAsync()
    {
        if (_selectedProject is null)
        {
            return new { projectId = (string?)null, projectName = (string?)null, capabilities = GrantedCapabilities() };
        }
        var (_, context) = await ResolveCurrentProjectContextAsync();
        return new { projectId = context.ProjectId, projectName = context.ProjectName, capabilities = GrantedCapabilities() };
    }

    private async Task<object> PrepareTaskBatchAsync(JsonElement payload)
    {
        var (owner, context) = await ResolveCurrentProjectContextAsync();
        var defaultsTask = _defaults.FetchAsync();
        var modelsTask = _models.FetchAvailableModelsAsync();
        await Task.WhenAll(defaultsTask, modelsTask);
        EnsureOwner(owner);
        var modelId = defaultsTask.Result.TaskRunnerDefaultModelConfigId;
        var model = modelsTask.Result.FirstOrDefault(value =>
            string.Equals(value.Id, modelId, StringComparison.Ordinal));
        if (string.IsNullOrWhiteSpace(modelId))
            throw new InvalidOperationException(Localization.Text(
                "请先在设置中选择 Task Runner 默认模型。",
                "Select a Task Runner default model in Settings first."));
        if (model is null || !model.TaskEnabled || !model.HasApiKey)
            throw new InvalidOperationException(Localization.Text(
                "Task Runner 默认模型已不可用、未启用 Task，或缺少凭据。",
                "The Task Runner default is unavailable, not enabled for Task, or missing credentials."));
        var request = JsonSerializer.Deserialize<PluginHostTaskBatchRequest>(payload.GetRawText(), JsonOptions)
            ?? throw new InvalidOperationException("Task batch request is empty.");
        var launch = _launch ?? throw new InvalidOperationException("Plugin host session is unavailable.");
        return await _taskRunner.PrepareBatchAsync(
            request,
            context,
            new PluginHostIdentity(
                launch.Application.PluginId,
                launch.Application.ComponentKey,
                launch.ReleaseId,
                launch.Version,
                launch.ArtifactSha256),
            modelId,
            default);
    }

    private async Task<object> ReadTaskStatusesAsync(JsonElement payload)
    {
        var (_, context) = await ResolveCurrentProjectContextAsync();
        var taskIds = ReadIdentifiers(payload, "taskIds", 200, allowEmpty: true);
        return await _taskRunner.TaskStatusesAsync(taskIds, context.ProjectId);
    }

    private async Task<object> OpenTaskWorkspaceAsync(JsonElement payload)
    {
        var (_, context) = await ResolveCurrentProjectContextAsync();
        var batchId = ValidIdentifier(ReadString(payload, "batchId"))
            ?? throw new InvalidOperationException("Task workspace batch ID is invalid.");
        var taskIds = ReadIdentifiers(payload, "taskIds", 200, allowEmpty: false);
        var tasks = await _taskRunner.TaskStatusesAsync(taskIds, context.ProjectId);
        _taskWorkspace = new TaskWorkspaceState(batchId, taskIds, context.ProjectId);
        RenderTaskWorkspace(tasks, null);
        TaskWorkspaceOverlay.Visibility = Visibility.Visible;
        return new { opened = true };
    }

    private async Task<(string Owner, ProjectContextSnapshot Context)> ResolveCurrentProjectContextAsync()
    {
        var project = _selectedProject
            ?? throw new InvalidOperationException(Localization.Text("插件未绑定项目。", "The plugin is not bound to a project."));
        var generation = Shell.AccountGeneration;
        var owner = RequireOwner(generation);
        var context = await _projects.ResolveContextAsync(owner, project.Id);
        RequireOwner(generation, owner);
        _projectContext = context;
        ProjectBadgeText.Text = context.ProjectName;
        return (owner, context);
    }

    private async Task RefreshTaskWorkspaceAsync()
    {
        if (_taskWorkspace is null) return;
        TaskWorkspaceProgress.IsActive = true;
        TaskWorkspaceMessage.Text = string.Empty;
        try
        {
            var (_, context) = await ResolveCurrentProjectContextAsync();
            if (!string.Equals(context.ProjectId, _taskWorkspace.ProjectId, StringComparison.Ordinal))
                throw new InvalidOperationException("The selected project changed.");
            var tasks = await _taskRunner.TaskStatusesAsync(_taskWorkspace.TaskIds, context.ProjectId);
            RenderTaskWorkspace(tasks, null);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            TaskWorkspaceMessage.Text = exception.Message;
        }
        catch (OperationCanceledException)
        {
            TaskWorkspaceMessage.Text = Localization.Text("账户或项目上下文已变化。", "The account or project context changed.");
        }
        finally
        {
            TaskWorkspaceProgress.IsActive = false;
        }
    }

    private void RenderTaskWorkspace(IReadOnlyList<PluginHostTaskReference> tasks, string? message)
    {
        TaskItems.Children.Clear();
        foreach (var task in tasks)
        {
            var color = StatusColor(task.Status);
            var content = new Grid { ColumnSpacing = 12 };
            content.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(12) });
            content.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            content.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            content.Children.Add(new Border
            {
                Width = 9,
                Height = 9,
                VerticalAlignment = VerticalAlignment.Center,
                Background = new SolidColorBrush(color),
                CornerRadius = new CornerRadius(5),
            });
            var text = new StackPanel { Spacing = 3 };
            text.Children.Add(new TextBlock { Text = task.Title, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold });
            text.Children.Add(new TextBlock
            {
                Text = task.TaskId,
                FontFamily = new FontFamily("Cascadia Mono, Consolas"),
                FontSize = 11,
                Foreground = ResourceBrush("ChatOSSecondaryTextBrush"),
            });
            Grid.SetColumn(text, 1);
            content.Children.Add(text);
            var status = new TextBlock
            {
                Text = StatusTitle(task.Status),
                VerticalAlignment = VerticalAlignment.Center,
                Foreground = new SolidColorBrush(color),
                FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            };
            Grid.SetColumn(status, 2);
            content.Children.Add(status);
            TaskItems.Children.Add(new Border
            {
                Padding = new Thickness(13),
                Background = ResourceBrush("ChatOSSidebarBackgroundBrush"),
                BorderBrush = ResourceBrush("ChatOSBorderBrush"),
                BorderThickness = new Thickness(1),
                CornerRadius = new CornerRadius(11),
                Child = content,
            });
        }
        StartTaskBatchButton.IsEnabled = tasks.Any(value => value.Status == "ready");
        TaskWorkspaceMessage.Text = message ?? string.Empty;
    }

    private async void OnRefreshTaskWorkspaceClicked(object sender, RoutedEventArgs e) =>
        await RefreshTaskWorkspaceAsync();

    private async void OnStartTaskBatchClicked(object sender, RoutedEventArgs e)
    {
        if (_taskWorkspace is null) return;
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = Localization.Text("开始执行这个任务批次？", "Start this task batch?"),
            Content = Localization.Text(
                "Task Runner 会按依赖关系调度待执行任务。关闭项目管理插件不会停止已经启动的任务。",
                "Task Runner will schedule ready tasks by dependency. Closing Project Management will not stop tasks already started."),
            PrimaryButtonText = StartExecutionText,
            CloseButtonText = Localization.Cancel,
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;
        TaskWorkspaceProgress.IsActive = true;
        StartTaskBatchButton.IsEnabled = false;
        try
        {
            var (_, context) = await ResolveCurrentProjectContextAsync();
            if (!string.Equals(context.ProjectId, _taskWorkspace.ProjectId, StringComparison.Ordinal))
                throw new InvalidOperationException("The selected project changed.");
            var results = await _taskRunner.StartBatchAsync(_taskWorkspace.TaskIds, context.ProjectId);
            var failures = results.Where(value => !value.Ok)
                .Select(value => value.Message)
                .Where(value => !string.IsNullOrWhiteSpace(value));
            await RefreshTaskWorkspaceAsync();
            var failureMessage = string.Join("；", failures.OfType<string>());
            if (failureMessage.Length > 0) TaskWorkspaceMessage.Text = failureMessage;
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            TaskWorkspaceMessage.Text = exception.Message;
        }
        catch (OperationCanceledException)
        {
            TaskWorkspaceMessage.Text = Localization.Text("账户或项目上下文已变化。", "The account or project context changed.");
        }
        finally
        {
            TaskWorkspaceProgress.IsActive = false;
        }
    }

    private void OnCloseTaskWorkspaceClicked(object sender, RoutedEventArgs e)
    {
        TaskWorkspaceOverlay.Visibility = Visibility.Collapsed;
        _taskWorkspace = null;
    }

    private void OnBackToApplicationsClicked(object sender, RoutedEventArgs e) => ShowCatalog();

    private void OnSwitchProjectClicked(object sender, RoutedEventArgs e)
    {
        _launchCancellation?.Cancel();
        _selectedProject = null;
        _projectContext = null;
        _launch = null;
        _allowedUrl = null;
        ShowProjectPicker();
    }

    private void OnReloadApplicationClicked(object sender, RoutedEventArgs e)
    {
        if (_webViewInitialized && _launch is not null) PluginWebView.CoreWebView2.Reload();
    }

    private async void OnRetryLaunchClicked(object sender, RoutedEventArgs e) =>
        await LaunchAsync(_selectedProject);

    private void ShowCatalog()
    {
        TaskWorkspaceOverlay.Visibility = Visibility.Collapsed;
        HostView.Visibility = Visibility.Collapsed;
        CatalogView.Visibility = Visibility.Visible;
    }

    private IReadOnlyList<string> GrantedCapabilities() => _launch?.Application.BridgeCapabilities
        .Where(HostCapabilities.Contains)
        .Distinct(StringComparer.Ordinal)
        .ToArray() ?? Array.Empty<string>();

    private bool Allows(Uri destination)
    {
        if (destination.AbsoluteUri == "about:blank") return true;
        if (_allowedUrl is null) return false;
        if (_allowedUrl.IsFile)
        {
            if (!destination.IsFile) return false;
            var comparison = OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;
            var root = Path.GetFullPath(Path.GetDirectoryName(_allowedUrl.LocalPath)!) + Path.DirectorySeparatorChar;
            var candidate = Path.GetFullPath(destination.LocalPath);
            return candidate.StartsWith(root, comparison);
        }
        return destination.Scheme == _allowedUrl.Scheme &&
            destination.Host == _allowedUrl.Host &&
            destination.Port == _allowedUrl.Port &&
            destination.Host == "127.0.0.1";
    }

    private void SendBridgeResponse(
        string requestId,
        bool ok,
        object result,
        string? errorCode = null,
        string? errorMessage = null) => SendBridgeMessage(new
    {
        type = "chatos.plugin_ui.response",
        protocol_version = 1,
        adapter_session_id = _adapterSessionId,
        host_session_nonce = _hostSessionNonce,
        request_id = requestId,
        ok,
        result,
        error_code = errorCode,
        error_message = errorMessage,
    });

    private void SendBridgeMessage(object value)
    {
        if (!_webViewInitialized) return;
        PluginWebView.CoreWebView2.PostWebMessageAsJson(JsonSerializer.Serialize(value, JsonOptions));
    }

    private string RequireOwner(long expectedGeneration)
    {
        var owner = Shell.CurrentOwnerUserId;
        if (string.IsNullOrWhiteSpace(owner) || Shell.AccountGeneration != expectedGeneration)
            throw new OperationCanceledException("The signed-in account changed.");
        return owner;
    }

    private void RequireOwner(long expectedGeneration, string expectedOwner)
    {
        if (Shell.AccountGeneration != expectedGeneration ||
            !string.Equals(Shell.CurrentOwnerUserId, expectedOwner, StringComparison.Ordinal))
            throw new OperationCanceledException("The signed-in account changed.");
    }

    private void EnsureOwner(string expectedOwner)
    {
        if (!string.Equals(Shell.CurrentOwnerUserId, expectedOwner, StringComparison.Ordinal))
            throw new OperationCanceledException("The signed-in account changed.");
    }

    private static bool RequiresProject(LocalPluginApplication application) =>
        application.ContextScope is "project" or "workspace";

    private static string? ReadString(JsonElement value, string name) =>
        value.TryGetProperty(name, out var property) && property.ValueKind == JsonValueKind.String
            ? property.GetString()
            : null;

    private static string? ValidIdentifier(string? value) =>
        !string.IsNullOrEmpty(value) && value.Length <= 256 && value == value.Trim() &&
        value.All(character => !char.IsControl(character))
            ? value
            : null;

    private static IReadOnlyList<string> ReadIdentifiers(
        JsonElement payload,
        string name,
        int maximum,
        bool allowEmpty)
    {
        if (!payload.TryGetProperty(name, out var values) || values.ValueKind != JsonValueKind.Array)
            throw new InvalidOperationException("Task Runner references are invalid.");
        var result = values.EnumerateArray().Select(value =>
            value.ValueKind == JsonValueKind.String ? ValidIdentifier(value.GetString()) : null).ToArray();
        if ((!allowEmpty && result.Length == 0) || result.Length > maximum || result.Any(value => value is null) ||
            result.Distinct(StringComparer.Ordinal).Count() != result.Length)
            throw new InvalidOperationException("Task Runner references are invalid.");
        return result.Select(value => value!).ToArray();
    }

    private static Brush ResourceBrush(string key) =>
        (Brush)Application.Current.Resources[key];

    private static Color StatusColor(string status) => status switch
    {
        "succeeded" => Color.FromArgb(255, 40, 160, 95),
        "failed" or "cancelled" => Color.FromArgb(255, 210, 69, 69),
        "blocked" => Color.FromArgb(255, 217, 119, 32),
        "running" or "queued" => Color.FromArgb(255, 37, 99, 235),
        _ => Color.FromArgb(255, 110, 110, 115),
    };

    private string StatusTitle(string status) => status switch
    {
        "draft" => Localization.Text("草稿", "Draft"),
        "ready" => Localization.Text("待执行", "Ready"),
        "queued" => Localization.Text("排队中", "Queued"),
        "running" => Localization.Text("运行中", "Running"),
        "succeeded" => Localization.Text("已完成", "Succeeded"),
        "failed" => Localization.Text("失败", "Failed"),
        "blocked" => Localization.Text("阻塞", "Blocked"),
        "cancelled" => Localization.Text("已取消", "Cancelled"),
        "archived" => Localization.Text("已归档", "Archived"),
        _ => status,
    };

    private sealed record TaskWorkspaceState(
        string BatchId,
        IReadOnlyList<string> TaskIds,
        string ProjectId);
}

public sealed class PluginApplicationCardViewModel
{
    public PluginApplicationCardViewModel(
        LocalPluginApplication application,
        LocalizationViewModel localization)
    {
        Application = application;
        Description = string.IsNullOrWhiteSpace(application.Description)
            ? localization.Text("插件应用", "Plugin application")
            : application.Description;
        RuntimeLabel = application.RequiresLocalRuntime
            ? localization.Text("本地服务", "Local service")
            : localization.Text("内嵌页面", "Embedded page");
        BrandBrush = ParseBrandBrush(application.BrandColor);
        Icon = string.IsNullOrWhiteSpace(application.IconPath)
            ? null
            : new BitmapImage(new Uri(application.IconPath));
    }

    public LocalPluginApplication Application { get; }
    public string Description { get; }
    public string RuntimeLabel { get; }
    public Brush BrandBrush { get; }
    public BitmapImage? Icon { get; }

    private static Brush ParseBrandBrush(string? value)
    {
        if (value is { Length: 7 } && value[0] == '#' &&
            byte.TryParse(value.AsSpan(1, 2), System.Globalization.NumberStyles.HexNumber, null, out var red) &&
            byte.TryParse(value.AsSpan(3, 2), System.Globalization.NumberStyles.HexNumber, null, out var green) &&
            byte.TryParse(value.AsSpan(5, 2), System.Globalization.NumberStyles.HexNumber, null, out var blue))
        {
            return new SolidColorBrush(Color.FromArgb(255, red, green, blue));
        }
        return new SolidColorBrush(Color.FromArgb(255, 37, 99, 235));
    }
}
