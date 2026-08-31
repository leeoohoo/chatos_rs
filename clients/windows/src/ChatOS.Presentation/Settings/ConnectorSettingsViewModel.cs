using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Settings;

public sealed partial class ConnectorSettingsViewModel : ObservableObject, IDisposable
{
    private readonly ILocalConnectorControlService _control;
    private readonly ILocalConnectorPairingTicketService _tickets;
    private readonly IUiDispatcher _dispatcher;
    private readonly LocalizationViewModel? _localization;
    private readonly TimeSpan _monitorInterval;
    private CancellationTokenSource? _monitorCancellation;

    public ConnectorSettingsViewModel(
        ILocalConnectorControlService control,
        ILocalConnectorPairingTicketService tickets,
        IUiDispatcher dispatcher,
        TimeSpan? monitorInterval = null,
        LocalizationViewModel? localization = null)
    {
        _control = control;
        _tickets = tickets;
        _dispatcher = dispatcher;
        _localization = localization;
        _monitorInterval = monitorInterval ?? TimeSpan.FromSeconds(2);
        Workspaces.CollectionChanged += (_, _) => OnPropertyChanged(nameof(CanPair));
        if (_localization is not null) _localization.PropertyChanged += OnLocalizationChanged;
    }

    public ObservableCollection<LocalConnectorWorkspaceDraft> Workspaces { get; } = [];

    public bool IsPaired => Status?.IsPaired == true;

    public string UsernameLabel => Status?.Username ?? "—";

    public string DeviceNameLabel => Status?.DeviceName ?? DeviceName;

    public string DeviceIdLabel => Status?.DeviceId ?? "—";

    public string GatewayLabel => Status?.GatewayBaseUrl ?? GatewayBaseUrl;

    public string LastSeenLabel => Status?.LastPongAt?.ToLocalTime().ToString("yyyy-MM-dd HH:mm:ss")
        ?? Status?.ConnectedAt?.ToLocalTime().ToString("yyyy-MM-dd HH:mm:ss")
        ?? "—";

    public IReadOnlyList<LocalConnectorWorkspaceStatus> PairedWorkspaces =>
        Status?.Workspaces ?? Array.Empty<LocalConnectorWorkspaceStatus>();

    public bool CanPair => !IsBusy && !IsPaired && Workspaces.Count > 0 &&
        Uri.TryCreate(GatewayBaseUrl, UriKind.Absolute, out var gateway) &&
        gateway.Scheme is "http" or "https" && DeviceName.Trim().Length > 0;

    public string StatusLabel => Status?.ConnectionPhase switch
    {
        "Connected" => L("已连接", "Connected"),
        "Connecting" => L("正在连接", "Connecting"),
        "WaitingToReconnect" => L("等待重连", "Waiting to reconnect"),
        "Suspended" => L("系统睡眠，连接已暂停", "Connection paused while the system sleeps"),
        "Stopped" => L("已配对，等待连接", "Paired, waiting to connect"),
        _ => IsPaired ? L("已配对", "Paired") : L("尚未配对", "Not paired"),
    };

    public string SectionLabel => L("本机连接器", "Local Connector");
    public string TitleLabel => L("Windows 本机 Connector", "Windows Local Connector");
    public string DescriptionLabel => L(
        "让 ChatOS 安全访问这台电脑上的项目、终端、Git 和运行环境。",
        "Give ChatOS secure access to projects, terminals, Git, and run environments on this computer.");
    public string RefreshStatusLabel => L("刷新连接状态", "Refresh connection status");
    public string GatewayAddressLabel => L("Gateway 地址", "Gateway address");
    public string DeviceNameInputLabel => L("设备名称", "Device name");
    public string DeviceNamePlaceholder => L("这台 Windows 电脑的名称", "Name of this Windows computer");
    public string AllowedWorkspacesLabel => L("允许访问的本机工作区", "Allowed local workspaces");
    public string WorkspaceBoundaryNotice => L("每个路径都会经过项目边界校验；重复路径只会保留一项。", "Each path is validated against project boundaries; duplicate paths are kept only once.");
    public string AddFolderLabel => L("添加文件夹", "Add folder");
    public string WorkspaceRequiredLabel => L("至少选择一个工作区后才能配对。", "Select at least one workspace before pairing.");
    public string PairDeviceLabel => L("配对这台电脑", "Pair this computer");
    public string ConnectionStatusLabel => L("连接状态", "Connection status");
    public string UserLabel => L("用户", "User");
    public string DeviceLabel => L("设备", "Device");
    public string LastConnectionLabel => L("最近连接", "Last connection");
    public string PairedWorkspacesLabel => L("已配对工作区", "Paired workspaces");
    public string DisconnectLabel => L("断开 Connector", "Disconnect Connector");

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsPaired))]
    [NotifyPropertyChangedFor(nameof(CanPair))]
    [NotifyPropertyChangedFor(nameof(StatusLabel))]
    [NotifyPropertyChangedFor(nameof(UsernameLabel))]
    [NotifyPropertyChangedFor(nameof(DeviceNameLabel))]
    [NotifyPropertyChangedFor(nameof(DeviceIdLabel))]
    [NotifyPropertyChangedFor(nameof(GatewayLabel))]
    [NotifyPropertyChangedFor(nameof(LastSeenLabel))]
    [NotifyPropertyChangedFor(nameof(PairedWorkspaces))]
    private LocalConnectorStatus? _status;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanPair))]
    [NotifyPropertyChangedFor(nameof(GatewayLabel))]
    private string _gatewayBaseUrl = Environment.GetEnvironmentVariable("CHATOS_LOCAL_CONNECTOR_CLOUD_BASE_URL")
        ?? "https://local-connector.jgoool.com";

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanPair))]
    [NotifyPropertyChangedFor(nameof(DeviceNameLabel))]
    private string _deviceName = Environment.MachineName;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanPair))]
    private bool _isBusy;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _actionMessage;

    public async Task OpenAsync(CancellationToken cancellationToken = default)
    {
        _monitorCancellation?.Cancel();
        _monitorCancellation?.Dispose();
        _monitorCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        await RefreshCoreAsync(_monitorCancellation.Token).ConfigureAwait(false);
        _ = MonitorAsync(_monitorCancellation.Token);
    }

    public void Close()
    {
        _monitorCancellation?.Cancel();
        _monitorCancellation?.Dispose();
        _monitorCancellation = null;
    }

    [RelayCommand]
    private Task RefreshAsync() => _monitorCancellation is null
        ? Task.CompletedTask
        : RefreshCoreAsync(_monitorCancellation.Token);

    [RelayCommand]
    private void AddWorkspace(LocalConnectorWorkspaceDraft? workspace)
    {
        if (workspace is null || string.IsNullOrWhiteSpace(workspace.AbsoluteRoot) ||
            Workspaces.Any(value => string.Equals(
                value.AbsoluteRoot,
                workspace.AbsoluteRoot,
                OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal)))
        {
            return;
        }

        Workspaces.Add(workspace);
    }

    [RelayCommand]
    private void RemoveWorkspace(LocalConnectorWorkspaceDraft? workspace)
    {
        if (workspace is not null) Workspaces.Remove(workspace);
    }

    [RelayCommand]
    private async Task PairAsync()
    {
        if (!CanPair || _monitorCancellation is null) return;
        var token = _monitorCancellation.Token;
        IsBusy = true;
        ErrorMessage = null;
        ActionMessage = null;
        try
        {
            var ticket = await _tickets.IssueAsync(token).ConfigureAwait(false);
            var status = await _control.PairAsync(
                new LocalConnectorPairingDraft(
                    GatewayBaseUrl.Trim(),
                    DeviceName.Trim(),
                    Workspaces.ToArray()),
                ticket,
                token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                Status = status;
                Workspaces.Clear();
                ActionMessage = L(
                    "这台 Windows 设备已完成配对，Connector 正在建立连接。",
                    "This Windows device is paired and the Connector is establishing a connection.");
            }, token).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsBusy = false).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private async Task DisconnectAsync()
    {
        if (!IsPaired || IsBusy || _monitorCancellation is null) return;
        var token = _monitorCancellation.Token;
        IsBusy = true;
        ErrorMessage = null;
        ActionMessage = null;
        try
        {
            await _control.DisconnectAsync(token).ConfigureAwait(false);
            await RefreshCoreAsync(token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => ActionMessage = L(
                    "本机 Connector 配对已清除。",
                    "Local Connector pairing was cleared."), token)
                .ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await RefreshCoreAsync(token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsBusy = false).ConfigureAwait(false);
        }
    }

    public void Dispose()
    {
        Close();
        if (_localization is not null) _localization.PropertyChanged -= OnLocalizationChanged;
    }

    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;

    private void OnLocalizationChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e) =>
        OnPropertyChanged(string.Empty);

    private async Task RefreshCoreAsync(CancellationToken token)
    {
        try
        {
            var status = await _control.GetStatusAsync(token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => Status = status, token).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
    }

    private async Task MonitorAsync(CancellationToken token)
    {
        while (!token.IsCancellationRequested)
        {
            try
            {
                await Task.Delay(_monitorInterval, token).ConfigureAwait(false);
                if (!IsBusy)
                {
                    await RefreshCoreAsync(token).ConfigureAwait(false);
                }
            }
            catch (OperationCanceledException) when (token.IsCancellationRequested)
            {
                break;
            }
        }
    }
}
