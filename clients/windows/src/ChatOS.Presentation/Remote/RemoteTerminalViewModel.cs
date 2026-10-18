using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Remote;

public sealed record RemoteTerminalLine(string Text, bool IsError, bool IsCommand);

public sealed partial class RemoteTerminalViewModel : ObservableObject, IDisposable
{
    private readonly IRemoteTerminalCommandService _service;
    private readonly IUiDispatcher _dispatcher;
    private readonly LocalizationViewModel? _localization;
    private CancellationTokenSource? _execution;
    private int _revision;
    private string? _pendingVerificationCommand;

    public RemoteTerminalViewModel(IRemoteTerminalCommandService service, IUiDispatcher dispatcher, LocalizationViewModel? localization = null)
    { _service = service; _dispatcher = dispatcher; _localization = localization; }

    public ObservableCollection<RemoteTerminalLine> Lines { get; } = [];
    public bool CanSubmit => !IsRunning && !string.IsNullOrWhiteSpace(Command) && ConnectionId is not null;
    public bool CanCancel => IsRunning;
    public bool HasVerificationChallenge => !string.IsNullOrWhiteSpace(VerificationPrompt);

    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSubmit))] private string? _connectionId;
    [ObservableProperty] private string _connectionName = string.Empty;
    [ObservableProperty] private string _workingDirectory = "~";
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSubmit))] private string _command = string.Empty;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(CanSubmit))] [NotifyPropertyChangedFor(nameof(CanCancel))] private bool _isRunning;
    [ObservableProperty] [NotifyPropertyChangedFor(nameof(HasVerificationChallenge))] private string? _verificationPrompt;
    [ObservableProperty] private string _verificationCode = string.Empty;
    [ObservableProperty] private string? _errorMessage;
    [ObservableProperty] private string _searchText = string.Empty;

    public void Open(RemoteConnection connection)
    {
        Cancel();
        _revision++;
        ConnectionId = connection.Id;
        ConnectionName = connection.Name;
        WorkingDirectory = string.IsNullOrWhiteSpace(connection.DefaultRemotePath) ? "~" : connection.DefaultRemotePath;
        Command = VerificationCode = string.Empty;
        VerificationPrompt = ErrorMessage = null;
        Lines.Clear();
        Lines.Add(new RemoteTerminalLine(L(
            $"已准备连接 {connection.Username}@{connection.Host}:{connection.Port}",
            $"Ready to connect to {connection.Username}@{connection.Host}:{connection.Port}"), false, false));
    }

    [RelayCommand]
    private async Task SubmitAsync()
    {
        var value = _pendingVerificationCommand ?? Command.Trim();
        if (IsRunning || ConnectionId is null || string.IsNullOrWhiteSpace(value)) return;
        if (_pendingVerificationCommand is not null && string.IsNullOrWhiteSpace(VerificationCode))
        {
            ErrorMessage = L("请输入验证码。", "Enter the verification code.");
            return;
        }
        var revision = _revision;
        var execution = new CancellationTokenSource();
        _execution = execution;
        IsRunning = true;
        ErrorMessage = null;
        VerificationPrompt = null;
        if (_pendingVerificationCommand is null)
        {
            Lines.Add(new RemoteTerminalLine($"{WorkingDirectory} $ {value}", false, true));
            Command = string.Empty;
        }
        try
        {
            var result = await _service.ExecuteAsync(ConnectionId, value, WorkingDirectory, VerificationCode, execution.Token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (revision != _revision) return;
                if (!string.IsNullOrWhiteSpace(result.Output)) Lines.Add(new RemoteTerminalLine(result.Output, false, false));
                if (!string.IsNullOrWhiteSpace(result.Error)) Lines.Add(new RemoteTerminalLine(result.Error, true, false));
                if (result.ExitCode != 0) Lines.Add(new RemoteTerminalLine(L(
                    $"进程退出码：{result.ExitCode}",
                    $"Process exit code: {result.ExitCode}"), true, false));
                WorkingDirectory = result.WorkingDirectory;
                _pendingVerificationCommand = null;
                VerificationCode = string.Empty;
            }).ConfigureAwait(false);
        }
        catch (RemoteVerificationRequiredException challenge)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (revision != _revision) return;
                _pendingVerificationCommand = value;
                VerificationPrompt = challenge.Prompt;
            }).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (execution.IsCancellationRequested)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (revision == _revision)
                    Lines.Add(new RemoteTerminalLine(L("命令已取消。", "Command cancelled."), true, false));
            }).ConfigureAwait(false);
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (revision == _revision) ErrorMessage = exception.Message;
            }).ConfigureAwait(false);
        }
        finally
        {
            if (ReferenceEquals(_execution, execution))
            {
                _execution = null;
                await _dispatcher.InvokeAsync(() => IsRunning = false).ConfigureAwait(false);
            }
            execution.Dispose();
        }
    }

    [RelayCommand]
    private void Cancel() => _execution?.Cancel();

    [RelayCommand]
    private void CancelVerification()
    {
        _pendingVerificationCommand = null;
        VerificationPrompt = null;
        VerificationCode = string.Empty;
        ErrorMessage = L("SSH 二次验证已取消。", "SSH verification was cancelled.");
    }

    [RelayCommand]
    private void ClearOutput() => Lines.Clear();

    public IReadOnlyList<int> FindMatches()
    {
        var query = SearchText.Trim();
        return query.Length == 0
            ? Array.Empty<int>()
            : Lines.Select((line, index) => (line, index))
                .Where(item => item.line.Text.Contains(query, StringComparison.OrdinalIgnoreCase))
                .Select(item => item.index)
                .ToArray();
    }

    public void Dispose() { Cancel(); _execution?.Dispose(); }

    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;
}
