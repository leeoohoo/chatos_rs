using System.Collections.ObjectModel;
using ChatOS.Api.WeChat;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.Settings;

public sealed partial class WeChatCompanionSettingsViewModel : ObservableObject, IDisposable
{
    private readonly IWeChatCompanionService _service;
    private readonly LocalizationViewModel _localization;
    private readonly IUiDispatcher _dispatcher;
    private readonly TimeProvider _timeProvider;
    private readonly SemaphoreSlim _mutationGate = new(1, 1);
    private CancellationTokenSource? _polling;

    public WeChatCompanionSettingsViewModel(
        IWeChatCompanionService service,
        LocalizationViewModel localization,
        IUiDispatcher dispatcher,
        TimeProvider? timeProvider = null)
    {
        _service = service;
        _localization = localization;
        _dispatcher = dispatcher;
        _timeProvider = timeProvider ?? TimeProvider.System;
        _localization.PropertyChanged += OnLocalizationChanged;
    }

    public ObservableCollection<WeChatSessionItemViewModel> Sessions { get; } = [];

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsBound))]
    [NotifyPropertyChangedFor(nameof(ShowUnbound))]
    private WeChatBindingStatus? _binding;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasTicket))]
    [NotifyPropertyChangedFor(nameof(ShowUnbound))]
    [NotifyPropertyChangedFor(nameof(TicketImageData))]
    private WeChatBindTicket? _ticket;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanConfirm))]
    [NotifyPropertyChangedFor(nameof(TicketHeading))]
    [NotifyPropertyChangedFor(nameof(TicketDescription))]
    private string _ticketStatus = string.Empty;

    [ObservableProperty]
    private string _expiryText = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(ShowUnbound))]
    private bool _isLoading;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanMutate))]
    [NotifyPropertyChangedFor(nameof(CanConfirm))]
    private bool _isMutating;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _notice;

    public bool IsBound => Binding?.Bound == true;
    public bool HasTicket => Ticket is not null;
    public bool ShowUnbound => !IsLoading && !IsBound && !HasTicket;
    public bool HasSessions => Sessions.Count > 0;
    public bool CanMutate => !IsMutating;
    public bool CanConfirm => !IsMutating && TicketStatus == "claimed";
    public byte[]? TicketImageData => Ticket?.CodeImageData;

    public string TicketHeading => TicketStatus == "claimed"
        ? L("微信设备请求绑定", "A WeChat device requested binding")
        : L("用微信扫描小程序码", "Scan with WeChat");

    public string TicketDescription => TicketStatus == "claimed"
        ? L(
            "请确认刚才是你本人扫码。确认后，该微信账号将能访问当前账号的 Companion 功能。",
            "Confirm that you scanned the code. The WeChat account will receive Companion access.")
        : L(
            "扫码后这里会出现二次确认。不要确认来自截图转发或陌生设备的请求。",
            "A second confirmation appears here. Never approve a forwarded screenshot or unknown device.");

    public async Task OpenAsync(CancellationToken cancellationToken = default)
    {
        if (IsLoading) return;
        await _dispatcher.InvokeAsync(() =>
        {
            IsLoading = true;
            ErrorMessage = null;
        }, cancellationToken).ConfigureAwait(false);

        try
        {
            var bindingTask = _service.GetBindingStatusAsync(cancellationToken);
            var sessionsTask = _service.ListClientSessionsAsync(cancellationToken);
            await Task.WhenAll(bindingTask, sessionsTask).ConfigureAwait(false);
            if (bindingTask.Result.Bound) StopPolling();
            await _dispatcher.InvokeAsync(() =>
            {
                Binding = bindingTask.Result;
                if (bindingTask.Result.Bound)
                {
                    Ticket = null;
                    TicketStatus = string.Empty;
                    ExpiryText = string.Empty;
                }
                ApplySessions(sessionsTask.Result);
                ErrorMessage = null;
            }, cancellationToken).ConfigureAwait(false);
            if (Ticket is { } ticket && TicketStatus == "issued") StartPolling(ticket);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsLoading = false).ConfigureAwait(false);
        }
    }

    public Task IssueTicketAsync(CancellationToken cancellationToken = default) =>
        MutateAsync(async token =>
        {
            StopPolling();
            var issued = await _service.IssueBindTicketAsync(token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                Ticket = issued;
                TicketStatus = "issued";
                Notice = null;
                UpdateExpiry(issued);
            }, token).ConfigureAwait(false);
            StartPolling(issued);
        }, cancellationToken);

    public Task ConfirmTicketAsync(CancellationToken cancellationToken = default) =>
        MutateAsync(async token =>
        {
            var current = Ticket;
            if (current is null || TicketStatus != "claimed") return;
            try
            {
                await _service.ConfirmBindTicketAsync(current.Id, token).ConfigureAwait(false);
            }
            catch (Exception confirmationError) when (confirmationError is not OperationCanceledException)
            {
                try
                {
                    var latest = await _service.GetBindingStatusAsync(token).ConfigureAwait(false);
                    if (!latest.Bound)
                        throw new InvalidOperationException(confirmationError.Message, confirmationError);
                }
                catch (OperationCanceledException)
                {
                    throw;
                }
                catch
                {
                    throw new InvalidOperationException(confirmationError.Message, confirmationError);
                }
            }

            StopPolling();
            var sessions = await _service.ListClientSessionsAsync(token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                Ticket = null;
                TicketStatus = string.Empty;
                ExpiryText = string.Empty;
                Binding = new WeChatBindingStatus(true, null, null);
                ApplySessions(sessions);
                Notice = L("微信小程序绑定已确认。", "WeChat Mini Program binding was confirmed.");
            }, token).ConfigureAwait(false);
        }, cancellationToken);

    public Task RevokeAsync(
        WeChatSessionItemViewModel session,
        CancellationToken cancellationToken = default) =>
        MutateAsync(async token =>
        {
            if (session.IsRevoked) return;
            await _service.RevokeClientSessionAsync(session.Id, token).ConfigureAwait(false);
            var sessions = await _service.ListClientSessionsAsync(token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                ApplySessions(sessions);
                Notice = L("已撤销该小程序登录。", "The Mini Program session was revoked.");
            }, token).ConfigureAwait(false);
        }, cancellationToken);

    public Task UnbindAsync(CancellationToken cancellationToken = default) =>
        MutateAsync(async token =>
        {
            await _service.UnbindAsync(token).ConfigureAwait(false);
            StopPolling();
            var sessions = await _service.ListClientSessionsAsync(token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                Ticket = null;
                TicketStatus = string.Empty;
                ExpiryText = string.Empty;
                Binding = new WeChatBindingStatus(false, null, null);
                ApplySessions(sessions);
                Notice = L(
                    "微信账号已解除绑定，所有小程序登录已撤销。",
                    "WeChat was unbound and all Mini Program sessions were revoked.");
            }, token).ConfigureAwait(false);
        }, cancellationToken);

    public void StopPolling()
    {
        var polling = Interlocked.Exchange(ref _polling, null);
        if (polling is null) return;
        polling.Cancel();
        polling.Dispose();
    }

    public void Dispose()
    {
        StopPolling();
        _localization.PropertyChanged -= OnLocalizationChanged;
        _mutationGate.Dispose();
    }

    private async Task MutateAsync(
        Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken)
    {
        if (!await _mutationGate.WaitAsync(0, cancellationToken).ConfigureAwait(false)) return;
        try
        {
            await _dispatcher.InvokeAsync(() =>
            {
                IsMutating = true;
                ErrorMessage = null;
                Notice = null;
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
            await _dispatcher.InvokeAsync(() => IsMutating = false).ConfigureAwait(false);
            _mutationGate.Release();
        }
    }

    private void StartPolling(WeChatBindTicket value)
    {
        StopPolling();
        var source = new CancellationTokenSource();
        _polling = source;
        _ = PollAsync(value, source);
    }

    private async Task PollAsync(WeChatBindTicket value, CancellationTokenSource source)
    {
        var delay = TimeSpan.FromSeconds(2);
        try
        {
            while (!source.IsCancellationRequested)
            {
                var expired = _timeProvider.GetUtcNow().ToUnixTimeSeconds() >= value.ExpiresAtUnix;
                await _dispatcher.InvokeAsync(() => UpdateExpiry(value), source.Token)
                    .ConfigureAwait(false);
                if (expired)
                {
                    await _dispatcher.InvokeAsync(() => TicketStatus = "expired", source.Token)
                        .ConfigureAwait(false);
                    return;
                }

                await Task.Delay(delay, _timeProvider, source.Token).ConfigureAwait(false);
                try
                {
                    var status = await _service.GetBindTicketStatusAsync(value.Id, source.Token)
                        .ConfigureAwait(false);
                    await _dispatcher.InvokeAsync(() =>
                    {
                        TicketStatus = status.Status;
                        ErrorMessage = null;
                        UpdateExpiry(value);
                    }, source.Token).ConfigureAwait(false);
                    delay = TimeSpan.FromSeconds(2);
                    if (status.Status is "claimed" or "expired") return;
                }
                catch (Exception exception) when (exception is not OperationCanceledException)
                {
                    await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                        .ConfigureAwait(false);
                    delay = TimeSpan.FromSeconds(Math.Min(delay.TotalSeconds * 2, 8));
                }
            }
        }
        catch (OperationCanceledException) when (source.IsCancellationRequested)
        {
        }
    }

    private void UpdateExpiry(WeChatBindTicket value)
    {
        var remaining = Math.Max(0, value.ExpiresAtUnix - _timeProvider.GetUtcNow().ToUnixTimeSeconds());
        ExpiryText = remaining > 0
            ? L($"小程序码将在 {remaining} 秒后过期", $"The code expires in {remaining} seconds")
            : L("小程序码已过期，请重新生成。", "The code expired. Generate another one.");
    }

    private void ApplySessions(IReadOnlyList<WeChatClientSession> values)
    {
        Sessions.Clear();
        foreach (var value in values)
        {
            Sessions.Add(new WeChatSessionItemViewModel(value, _localization));
        }
        OnPropertyChanged(nameof(HasSessions));
    }

    private void OnLocalizationChanged(
        object? sender,
        System.ComponentModel.PropertyChangedEventArgs e)
    {
        foreach (var session in Sessions) session.ApplyLocalization(_localization);
        if (Ticket is not null) UpdateExpiry(Ticket);
        OnPropertyChanged(string.Empty);
    }

    private string L(string chinese, string english) => _localization.Text(chinese, english);
}

public sealed partial class WeChatSessionItemViewModel : ObservableObject
{
    public WeChatSessionItemViewModel(
        WeChatClientSession session,
        LocalizationViewModel localization)
    {
        Session = session;
        ApplyLocalization(localization);
    }

    public WeChatClientSession Session { get; }
    public string Id => Session.Id;
    public bool IsRevoked => Session.RevokedAt is not null;

    [ObservableProperty]
    private string _title = string.Empty;

    [ObservableProperty]
    private string _detail = string.Empty;

    [ObservableProperty]
    private string _statusLabel = string.Empty;

    public void ApplyLocalization(LocalizationViewModel localization)
    {
        Title = localization.Text("微信小程序", "WeChat Mini Program");
        var lastSeen = DateTimeOffset.TryParse(Session.LastSeenAt, out var parsed)
            ? parsed.ToLocalTime().ToString("g")
            : Session.LastSeenAt;
        Detail = localization.Text($"最近活动：{lastSeen}", $"Last active: {lastSeen}");
        StatusLabel = IsRevoked
            ? localization.Text("已撤销", "Revoked")
            : localization.Text("撤销", "Revoke");
    }
}
