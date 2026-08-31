using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Remote;

public sealed record RemoteWorkspaceOption(string Id, string Label, string DeviceId);

public sealed partial class RemoteConnectionsViewModel : ObservableObject
{
    private readonly IRemoteConnectionService _service;
    private readonly ILocalConnectorControlService _connector;
    private readonly IUiDispatcher _dispatcher;
    private readonly LocalizationViewModel? _localization;

    public RemoteConnectionsViewModel(
        IRemoteConnectionService service,
        ILocalConnectorControlService connector,
        IUiDispatcher dispatcher,
        LocalizationViewModel? localization = null)
    {
        _service = service;
        _connector = connector;
        _dispatcher = dispatcher;
        _localization = localization;
    }

    public ObservableCollection<RemoteConnection> Connections { get; } = [];
    public ObservableCollection<RemoteWorkspaceOption> Workspaces { get; } = [];
    public bool IsEditingExisting => SelectedConnection is not null;
    public bool HasVerificationChallenge => !string.IsNullOrWhiteSpace(VerificationPrompt);
    public bool CanSave => !IsBusy && Host.Trim().Length > 0 && Username.Trim().Length > 0 &&
        Port is >= 1 and <= 65535 && SelectedWorkspace is not null && HasTargetCredentials && HasJumpCredentials;

    private bool HasTargetCredentials => AuthenticationType switch
    {
        RemoteAuthenticationType.Password => Password.Trim().Length > 0 ||
            (SelectedConnection?.AuthenticationType == RemoteAuthenticationType.Password && SelectedConnection.HasPassword),
        RemoteAuthenticationType.PrivateKeyCertificate =>
            (PrivateKeyPath.Trim().Length > 0 || SelectedConnection?.HasPrivateKeyPath == true) &&
            (CertificatePath.Trim().Length > 0 || SelectedConnection?.HasCertificatePath == true),
        _ => PrivateKeyPath.Trim().Length > 0 || SelectedConnection?.HasPrivateKeyPath == true,
    };

    private bool HasJumpCredentials => !JumpEnabled ||
        !string.IsNullOrWhiteSpace(JumpConnectionId) ||
        (JumpHost.Trim().Length > 0 && JumpUsername.Trim().Length > 0 &&
         (JumpPassword.Trim().Length > 0 || JumpPrivateKeyPath.Trim().Length > 0 ||
          SelectedConnection?.HasJumpPassword == true || SelectedConnection?.HasJumpPrivateKeyPath == true));

    [ObservableProperty] [NotifyPropertyChangedFor(nameof(IsEditingExisting))] [NotifyPropertyChangedFor(nameof(CanSave))]
    private RemoteConnection? _selectedConnection;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private RemoteWorkspaceOption? _selectedWorkspace;
    [ObservableProperty] private string _name = string.Empty;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private string _host = string.Empty;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private int _port = 22;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private string _username = string.Empty;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private RemoteAuthenticationType _authenticationType = RemoteAuthenticationType.PrivateKey;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private string _password = string.Empty;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private string _privateKeyPath = string.Empty;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private string _certificatePath = string.Empty;
    [ObservableProperty] private string _defaultRemotePath = string.Empty;
    [ObservableProperty] private RemoteHostKeyPolicy _hostKeyPolicy = RemoteHostKeyPolicy.Strict;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private bool _jumpEnabled;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private string? _jumpConnectionId;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private string _jumpHost = string.Empty;
    [ObservableProperty] private int _jumpPort = 22;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private string _jumpUsername = string.Empty;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private string _jumpPassword = string.Empty;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private string _jumpPrivateKeyPath = string.Empty;
    [ObservableProperty] private string _jumpCertificatePath = string.Empty;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSave))] private bool _isBusy;
    [ObservableProperty] private string? _errorMessage;
    [ObservableProperty] private string? _actionMessage;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(HasVerificationChallenge))] private string? _verificationPrompt;
    [ObservableProperty] private string _verificationCode = string.Empty;

    public async Task OpenAsync(CancellationToken cancellationToken = default)
    {
        var selectedId = SelectedConnection?.Id;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            var connectionsTask = _service.ListAsync(cancellationToken);
            var connectorTask = _connector.GetStatusAsync(cancellationToken);
            await Task.WhenAll(connectionsTask, connectorTask).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                Connections.Clear();
                foreach (var value in connectionsTask.Result.OrderBy(static value => value.Name)) Connections.Add(value);
                Workspaces.Clear();
                var status = connectorTask.Result;
                foreach (var workspace in status.Workspaces)
                    Workspaces.Add(new RemoteWorkspaceOption(workspace.Id, $"{workspace.Alias} · {workspace.AbsoluteRoot}", status.DeviceId ?? string.Empty));
                var selected = selectedId is null ? null : Connections.FirstOrDefault(value => value.Id == selectedId);
                if (selected is null) New(); else Edit(selected);
            }, cancellationToken).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
        finally { await _dispatcher.InvokeAsync(() => IsBusy = false).ConfigureAwait(false); }
    }

    [RelayCommand]
    private void New()
    {
        SelectedConnection = null;
        Name = Host = Username = Password = PrivateKeyPath = CertificatePath = DefaultRemotePath = string.Empty;
        Port = 22;
        AuthenticationType = RemoteAuthenticationType.PrivateKey;
        HostKeyPolicy = RemoteHostKeyPolicy.Strict;
        JumpEnabled = false;
        JumpConnectionId = null;
        JumpHost = JumpUsername = JumpPassword = JumpPrivateKeyPath = JumpCertificatePath = string.Empty;
        JumpPort = 22;
        SelectedWorkspace ??= Workspaces.FirstOrDefault();
        ClearMessages();
    }

    [RelayCommand]
    private void Edit(RemoteConnection? connection)
    {
        if (connection is null) return;
        SelectedConnection = connection;
        Name = connection.Name;
        Host = connection.Host;
        Port = connection.Port;
        Username = connection.Username;
        AuthenticationType = connection.AuthenticationType;
        Password = PrivateKeyPath = CertificatePath = string.Empty;
        DefaultRemotePath = connection.DefaultRemotePath ?? string.Empty;
        HostKeyPolicy = connection.HostKeyPolicy;
        SelectedWorkspace = Workspaces.FirstOrDefault(value => value.Id == connection.LocalConnectorWorkspaceId);
        JumpEnabled = connection.JumpEnabled;
        JumpConnectionId = connection.JumpConnectionId;
        JumpHost = connection.JumpHost ?? string.Empty;
        JumpPort = connection.JumpPort ?? 22;
        JumpUsername = connection.JumpUsername ?? string.Empty;
        JumpPassword = JumpPrivateKeyPath = JumpCertificatePath = string.Empty;
        ClearMessages();
    }

    [RelayCommand]
    private async Task SaveAsync()
    {
        if (!CanSave) return;
        await MutateAsync(async token =>
        {
            var saved = SelectedConnection is null
                ? await _service.CreateAsync(Draft(), token).ConfigureAwait(false)
                : await _service.UpdateAsync(SelectedConnection.Id, Draft(), token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                var existing = Connections.FirstOrDefault(value => value.Id == saved.Id);
                if (existing is null) Connections.Add(saved); else Connections[Connections.IndexOf(existing)] = saved;
                Edit(saved);
                ActionMessage = L(
                    "远端连接已保存；密码和密钥路径仅保存在本机凭据库。",
                    "The remote connection was saved; passwords and key paths are stored only in the local credential vault.");
            }, token).ConfigureAwait(false);
        }).ConfigureAwait(false);
    }

    [RelayCommand]
    private Task TestAsync() => TestCoreAsync(VerificationCode);

    [RelayCommand]
    private async Task DeleteAsync(RemoteConnection? connection)
    {
        if (connection is null) return;
        await MutateAsync(async token =>
        {
            await _service.DeleteAsync(connection.Id, token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                Connections.Remove(connection);
                if (SelectedConnection?.Id == connection.Id) New();
                ActionMessage = L(
                    "远端连接及本机凭据已删除。",
                    "The remote connection and local credentials were deleted.");
            }, token).ConfigureAwait(false);
        }).ConfigureAwait(false);
    }

    private async Task TestCoreAsync(string? code)
    {
        if (!CanSave) return;
        VerificationPrompt = null;
        await MutateAsync(async token =>
        {
            try
            {
                var result = await _service.TestDraftAsync(Draft(), code, token).ConfigureAwait(false);
                await _dispatcher.InvokeAsync(() =>
                {
                    VerificationCode = string.Empty;
                    ActionMessage = result.Message ?? (result.Success
                        ? L("SSH 连接成功。", "SSH connection succeeded.")
                        : L("SSH 连接失败。", "SSH connection failed."));
                }, token).ConfigureAwait(false);
            }
            catch (RemoteVerificationRequiredException challenge)
            {
                await _dispatcher.InvokeAsync(() => VerificationPrompt = challenge.Prompt, token).ConfigureAwait(false);
            }
        }).ConfigureAwait(false);
    }

    private RemoteConnectionDraft Draft()
    {
        var jump = Connections.FirstOrDefault(value => value.Id == JumpConnectionId);
        return new RemoteConnectionDraft(
            Clean(Name), Host.Trim(), Port, Username.Trim(), AuthenticationType,
            Clean(Password), Clean(PrivateKeyPath), Clean(CertificatePath), Clean(DefaultRemotePath),
            HostKeyPolicy, SelectedWorkspace?.DeviceId ?? string.Empty, SelectedWorkspace?.Id ?? string.Empty,
            JumpEnabled, Clean(JumpConnectionId), jump?.Host ?? Clean(JumpHost), jump?.Port ?? JumpPort,
            jump?.Username ?? Clean(JumpUsername), Clean(JumpPrivateKeyPath), Clean(JumpCertificatePath),
            Clean(JumpPassword), SelectedConnection?.Id);
    }

    private async Task MutateAsync(Func<CancellationToken, Task> action)
    {
        if (IsBusy) return;
        IsBusy = true;
        ErrorMessage = null;
        ActionMessage = null;
        try { await action(CancellationToken.None).ConfigureAwait(false); }
        catch (Exception exception) when (exception is not OperationCanceledException and not RemoteVerificationRequiredException)
        { await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false); }
        finally { await _dispatcher.InvokeAsync(() => IsBusy = false).ConfigureAwait(false); }
    }

    private void ClearMessages() { ErrorMessage = ActionMessage = VerificationPrompt = null; VerificationCode = string.Empty; }
    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;
    private static string? Clean(string? value) => string.IsNullOrWhiteSpace(value) ? null : value.Trim();
}
