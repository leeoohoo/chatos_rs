using ChatOS.Connector.Terminal;
using ChatOS.Core.Domain;
using ChatOS.Desktop.AppShell;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Terminal;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Windows.System;

namespace ChatOS.Desktop.Features.Terminal;

public sealed partial class LocalTerminalPage : UserControl
{
    private const int MaximumOutputCharacters = 200_000;
    private readonly TerminalSessionManager _sessions;
    private readonly TerminalPlainTextDecoder _outputDecoder = new();
    private ITerminalSession? _session;

    public LocalTerminalPage(
        TerminalSessionManager sessions,
        LocalizationViewModel localization)
    {
        _sessions = sessions;
        Localization = localization;
        InitializeComponent();
    }

    public LocalizationViewModel Localization { get; }

    public event EventHandler? CloseRequested;

    public event EventHandler? NewTerminalRequested;

    public async Task OpenAsync(
        ShellResourceViewModel resource,
        CancellationToken cancellationToken = default)
    {
        if (resource.Kind is not WorkspaceResourceKind.LocalTerminal ||
            string.IsNullOrWhiteSpace(resource.WorkspaceId) ||
            string.IsNullOrWhiteSpace(resource.AbsoluteRoot))
        {
            throw new ArgumentException("Local terminal resource identity is incomplete.", nameof(resource));
        }

        await CloseSessionAsync();
        TitleText.Text = resource.Title;
        PathText.Text = resource.AbsoluteRoot;
        _outputDecoder.Reset();
        OutputText.Text = string.Empty;
        var identity = new TerminalSessionIdentity(
            resource.Id,
            resource.WorkspaceId,
            resource.AbsoluteRoot,
            resource.AbsoluteRoot);
        _session = await _sessions.EnsureSessionAsync(
            identity,
            TerminalSize.Normalize(120, 36),
            cancellationToken);
        _session.EventReceived += OnTerminalEvent;
        ConnectionStatusText.Text = Localization.Text("已连接", "Connected");
        InputText.Focus(FocusState.Programmatic);
    }

    private void OnTerminalEvent(object? sender, TerminalEvent e)
    {
        if (sender is not ITerminalSession sourceSession) return;
        _ = DispatcherQueue.TryEnqueue(() =>
        {
            if (!ReferenceEquals(_session, sourceSession)) return;
            if (e.Kind is TerminalEventKind.Output && e.Data is { Length: > 0 })
            {
                AppendOutput(e.Data);
            }
            else if (e.Kind is TerminalEventKind.Error && e.Data is { Length: > 0 })
            {
                AppendOutput($"\r\n[error] {e.Data}\r\n");
            }
            else if (e.Kind is TerminalEventKind.Exit)
            {
                AppendOutput($"\r\n[process exited: {e.ExitCode}]\r\n");
            }
        });
    }

    private async void OnInputKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key == VirtualKey.Enter)
        {
            e.Handled = true;
            await SendAsync();
        }
    }

    private async void OnSendClicked(object sender, RoutedEventArgs e) => await SendAsync();

    private void OnSearchKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key != VirtualKey.Enter) return;
        e.Handled = true;
        FindNextOutputMatch();
    }

    private void OnSearchClicked(object sender, RoutedEventArgs e) => FindNextOutputMatch();

    private void OnSearchPreviousClicked(object sender, RoutedEventArgs e) =>
        FindPreviousOutputMatch();

    private void OnNewTerminalClicked(object sender, RoutedEventArgs e) =>
        NewTerminalRequested?.Invoke(this, EventArgs.Empty);

    private void FindNextOutputMatch()
    {
        var query = SearchText.Text.Trim();
        if (query.Length == 0 || OutputText.Text.Length == 0) return;

        var nextStart = Math.Min(
            OutputText.Text.Length,
            OutputText.SelectionStart + Math.Max(OutputText.SelectionLength, 1));
        var index = OutputText.Text.IndexOf(query, nextStart, StringComparison.CurrentCultureIgnoreCase);
        if (index < 0)
        {
            index = OutputText.Text.IndexOf(query, StringComparison.CurrentCultureIgnoreCase);
        }
        if (index < 0) return;

        OutputText.Focus(FocusState.Programmatic);
        OutputText.SelectionStart = index;
        OutputText.SelectionLength = query.Length;
    }

    private void FindPreviousOutputMatch()
    {
        var query = SearchText.Text.Trim();
        var output = OutputText.Text;
        if (query.Length == 0 || output.Length == 0) return;

        var searchStart = OutputText.SelectionStart > 0
            ? OutputText.SelectionStart - 1
            : output.Length - 1;
        var index = output.LastIndexOf(
            query,
            searchStart,
            StringComparison.CurrentCultureIgnoreCase);
        if (index < 0)
        {
            index = output.LastIndexOf(query, StringComparison.CurrentCultureIgnoreCase);
        }
        if (index < 0) return;

        OutputText.Focus(FocusState.Programmatic);
        OutputText.SelectionStart = index;
        OutputText.SelectionLength = query.Length;
    }

    private async Task SendAsync()
    {
        var value = InputText.Text;
        if (_session is null || string.IsNullOrWhiteSpace(value))
        {
            return;
        }
        InputText.Text = string.Empty;
        try
        {
            await _session.WriteAsync(value + "\r");
        }
        catch (Exception exception)
        {
            AppendOutput($"\r\n[error] {exception.Message}\r\n");
        }
    }

    private async void OnStopClicked(object sender, RoutedEventArgs e) => await CloseSessionAsync();

    private async void OnCloseClicked(object sender, RoutedEventArgs e)
    {
        await CloseSessionAsync();
        CloseRequested?.Invoke(this, EventArgs.Empty);
    }

    public async Task CloseSessionAsync()
    {
        if (_session is null)
        {
            return;
        }
        var session = _session;
        _session = null;
        ConnectionStatusText.Text = Localization.Text("已停止", "Stopped");
        session.EventReceived -= OnTerminalEvent;
        await _sessions.CloseAsync(session.Identity.SessionId, CancellationToken.None);
    }

    private void AppendOutput(string value)
    {
        var plainText = _outputDecoder.Decode(value);
        if (plainText.Length == 0) return;
        var combined = OutputText.Text + plainText;
        if (combined.Length > MaximumOutputCharacters)
        {
            combined = combined[^MaximumOutputCharacters..];
        }
        OutputText.Text = combined;
        OutputText.SelectionStart = OutputText.Text.Length;
    }
}
