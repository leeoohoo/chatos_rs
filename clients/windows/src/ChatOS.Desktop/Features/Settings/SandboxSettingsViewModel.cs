using ChatOS.Connector.NetworkGuard;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Sandbox;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.Settings;

public sealed partial class SandboxSettingsViewModel : ObservableObject
{
    private readonly IConnectorSandboxSettingsStore _store;
    private readonly LocalizationViewModel _localization;
    private readonly IUiDispatcher _dispatcher;
    private readonly IControlledNetworkGuardClient? _networkGuard;
    private readonly IConnectorControlledNetworkReadinessService? _controlledNetworkReadiness;
    private readonly SemaphoreSlim _operationGate = new(1, 1);

    public SandboxSettingsViewModel(
        IConnectorSandboxSettingsStore store,
        LocalizationViewModel localization,
        IUiDispatcher dispatcher,
        IControlledNetworkGuardClient? networkGuard = null,
        IConnectorControlledNetworkReadinessService? controlledNetworkReadiness = null)
    {
        _store = store;
        _localization = localization;
        _dispatcher = dispatcher;
        _networkGuard = networkGuard;
        _controlledNetworkReadiness = controlledNetworkReadiness;
        _localization.PropertyChanged += (_, _) => OnPropertyChanged(string.Empty);
    }

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(Description))]
    private bool _isEnabled = true;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(Description))]
    private ConnectorSandboxPermissionProfile _permissionProfile =
        ConnectorSandboxPermissionProfile.WorkspaceWrite;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(Description))]
    private ConnectorSandboxNetworkAccess _networkAccess =
        ConnectorSandboxNetworkAccess.Disabled;

    [ObservableProperty]
    private bool _isBusy;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _actionMessage;

    [ObservableProperty]
    private bool _isControlledNetworkAvailable;

    [ObservableProperty]
    private string? _controlledNetworkStatusMessage;

    public string SectionDescription => _localization.Text(
        "命令进程使用 Windows AppContainer 与工作区 ACL 强制限制文件和网络访问。",
        "Command processes use Windows AppContainer and workspace ACLs to enforce file and network boundaries.");

    public string CommandSandboxLabel => _localization.Text("命令沙箱", "Command sandbox");
    public string FilePermissionLabel => _localization.Text("文件权限", "File permissions");
    public string NetworkPermissionLabel => _localization.Text("网络权限", "Network permissions");
    public string FullAccessLabel => _localization.Text("完全访问", "Full access");
    public string AppContainerLabel => "AppContainer";
    public string ReadOnlyLabel => _localization.Text("项目只读", "Project read-only");
    public string WorkspaceWriteLabel => _localization.Text("仅项目可写", "Project write only");
    public string NetworkDisabledLabel => _localization.Text("禁止网络", "Network disabled");
    public string HostNetworkLabel => _localization.Text("允许主机网络", "Allow host network");
    public string ControlledNetworkLabel => _localization.Text("仅允许签名域名", "Signed domains only");

    public string Description
    {
        get
        {
            if (!IsEnabled || PermissionProfile is ConnectorSandboxPermissionProfile.FullAccess)
            {
                return _localization.Text(
                    "完全访问会绕过 AppContainer，命令拥有当前 Windows 用户的文件和网络权限。",
                    "Full access bypasses AppContainer and gives commands the current Windows user's file and network permissions.");
            }

            var files = PermissionProfile is ConnectorSandboxPermissionProfile.ReadOnly
                ? _localization.Text("项目只读", "read-only project access")
                : _localization.Text("仅项目可写", "writes limited to the project");
            var network = NetworkAccess switch
            {
                ConnectorSandboxNetworkAccess.Host =>
                    _localization.Text("允许主机网络", "host network allowed"),
                ConnectorSandboxNetworkAccess.Controlled =>
                    _localization.Text("仅允许服务端签名域名", "only server-signed domains allowed"),
                _ => _localization.Text("禁止网络", "network disabled"),
            };
            return _localization.Text(
                $"强制边界：{files}，{network}；进程树在任务结束、取消或超时时整体回收。",
                $"Enforced boundary: {files}, {network}; the full process tree is reclaimed on completion, cancellation, or timeout.");
        }
    }

    public async Task OpenAsync(CancellationToken cancellationToken = default) =>
        await RunAsync(async token =>
        {
            var readiness = await CheckCombinedReadinessAsync(token).ConfigureAwait(false);
            var settings = await _store.LoadAsync(token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                IsControlledNetworkAvailable = readiness.Available;
                ControlledNetworkStatusMessage = readiness.Message;
                Apply(settings);
            }, token).ConfigureAwait(false);
        }, cancellationToken).ConfigureAwait(false);

    public async Task RefreshReadinessAsync(CancellationToken cancellationToken = default) =>
        await RunAsync(async token =>
        {
            var readiness = await CheckCombinedReadinessAsync(token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                IsControlledNetworkAvailable = readiness.Available;
                ControlledNetworkStatusMessage = readiness.Message;
            }, token).ConfigureAwait(false);
        }, cancellationToken).ConfigureAwait(false);

    public async Task SaveAsync(
        bool fullAccessConfirmed,
        CancellationToken cancellationToken = default) =>
        await RunAsync(async token =>
        {
            if (NetworkAccess is ConnectorSandboxNetworkAccess.Controlled)
            {
                var readiness = _networkGuard is null
                    ? new NetworkGuardReadiness(NetworkGuardReadinessState.ServiceUnavailable)
                    : await _networkGuard.CheckReadinessAsync(token).ConfigureAwait(false);
                if (!readiness.IsReady)
                {
                    throw new InvalidOperationException(_localization.Text(
                        $"受控域名网络组件不可用（{readiness.State}）。",
                        $"Controlled-domain networking is unavailable ({readiness.State})."));
                }
                var cloudReadiness = await CheckCloudReadinessAsync(token).ConfigureAwait(false);
                if (!cloudReadiness.Available)
                {
                    throw new InvalidOperationException(ControlledNetworkCloudError(cloudReadiness.State));
                }
                await _dispatcher.InvokeAsync(() =>
                {
                    IsControlledNetworkAvailable = true;
                    ControlledNetworkStatusMessage = null;
                }, token).ConfigureAwait(false);
            }
            if ((!IsEnabled || PermissionProfile is ConnectorSandboxPermissionProfile.FullAccess) &&
                !fullAccessConfirmed)
            {
                throw new InvalidOperationException(_localization.Text(
                    "完全访问需要明确确认。",
                    "Full access requires explicit confirmation."));
            }

            var settings = new ConnectorSandboxSettings(
                IsEnabled,
                PermissionProfile,
                NetworkAccess).Normalize();
            await _store.SaveAsync(settings, token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                Apply(settings);
                ActionMessage = _localization.Text(
                    "沙箱设置已保存，新启动的命令将使用该边界。",
                    "Sandbox settings were saved. Newly launched commands will use this boundary.");
            }, token).ConfigureAwait(false);
        }, cancellationToken).ConfigureAwait(false);

    private async Task<(bool Available, string? Message)> CheckCombinedReadinessAsync(
        CancellationToken cancellationToken)
    {
        var local = _networkGuard is null
            ? new NetworkGuardReadiness(NetworkGuardReadinessState.ServiceUnavailable)
            : await _networkGuard.CheckReadinessAsync(cancellationToken).ConfigureAwait(false);
        if (!local.IsReady)
        {
            return (false, _localization.Text(
                $"本机受控网络组件不可用（{local.State}）。",
                $"The local Controlled-network component is unavailable ({local.State})."));
        }

        var cloud = await CheckCloudReadinessAsync(cancellationToken).ConfigureAwait(false);
        return cloud.Available
            ? (true, null)
            : (false, ControlledNetworkCloudError(cloud.State));
    }

    private async Task<ConnectorControlledNetworkReadiness> CheckCloudReadinessAsync(
        CancellationToken cancellationToken)
    {
        if (_controlledNetworkReadiness is null)
        {
            return new ConnectorControlledNetworkReadiness(true, "not_checked", null, 0);
        }

        try
        {
            return await _controlledNetworkReadiness.CheckAsync(cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
            throw;
        }
        catch
        {
            return new ConnectorControlledNetworkReadiness(false, "gateway_unavailable", null, 0);
        }
    }

    private string ControlledNetworkCloudError(string state) => state switch
    {
        "connector_not_paired" => _localization.Text(
            "请先配对并连接本机 Connector。",
            "Pair and connect the local Connector first."),
        "windows_sid_not_registered" => _localization.Text(
            "Connector 尚未完成 Windows 身份注册，请等待重连后重试。",
            "The Connector has not registered its Windows identity yet. Retry after it reconnects."),
        "managed_policy_not_configured" or "managed_allowlist_not_configured" =>
            _localization.Text(
                "服务端尚未配置受控网络域名白名单。",
                "The server has no managed Controlled-network domain allowlist."),
        "managed_policy_invalid" or "managed_policy_not_compilable" =>
            _localization.Text(
                "服务端受控网络策略无效，无法安全签发。",
                "The managed Controlled-network policy is invalid and cannot be safely issued."),
        "signer_not_configured" => _localization.Text(
            "服务端尚未配置受控网络签名。",
            "The server has no Controlled-network signer configured."),
        _ => _localization.Text(
            "暂时无法确认服务端受控网络策略。",
            "The server-side Controlled-network policy could not be verified."),
    };

    private void Apply(ConnectorSandboxSettings settings)
    {
        settings = settings.Normalize();
        IsEnabled = settings.Enabled;
        PermissionProfile = settings.PermissionProfile;
        NetworkAccess = settings.NetworkAccess;
    }

    private async Task RunAsync(
        Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken)
    {
        await _operationGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await _dispatcher.InvokeAsync(() =>
            {
                IsBusy = true;
                ErrorMessage = null;
                ActionMessage = null;
            }, cancellationToken).ConfigureAwait(false);
            await operation(cancellationToken).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsBusy = false).ConfigureAwait(false);
            _operationGate.Release();
        }
    }
}
