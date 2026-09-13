using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Chat;

public sealed partial class LocalAgentRunControlViewModel : ObservableObject
{
    private readonly ILocalAgentRunControlService _service;
    private readonly Func<Task> _onChanged;
    private readonly LocalizationViewModel? _localization;

    public LocalAgentRunControlViewModel(LocalAgentRunControlState state,
        ILocalAgentRunControlService service, Func<Task> onChanged,
        LocalizationViewModel? localization = null)
    {
        State = state;
        _service = service;
        _onChanged = onChanged;
        _localization = localization;
    }

    public LocalAgentRunControlState State { get; }
    public string RunId => State.RunId;
    public bool CanPause => State.CanPause;
    public bool CanResume => State.CanResume;
    public bool CanCancel => State.CanCancel;
    public bool RequiresAttention => State.Status is LocalAgentRunStatus.NeedsReview
        or LocalAgentRunStatus.Paused;
    public string StatusLabel => State.Status switch
    {
        LocalAgentRunStatus.NeedsReview => L("需要人工复核", "Needs review"),
        LocalAgentRunStatus.Paused => L("已暂停", "Paused"),
        LocalAgentRunStatus.WaitingToolResult => L("等待工具结果", "Waiting for tool"),
        LocalAgentRunStatus.ModelRunning => L("AI 正在执行", "AI is running"),
        _ => State.Status.ToString(),
    };
    public string Detail => State.ReviewReason ?? L(
        $"第 {State.Iteration} 轮 · 重试 {State.RetryCount} 次",
        $"Iteration {State.Iteration} · {State.RetryCount} retries");
    public string PauseLabel => L("暂停", "Pause");
    public string ResumeLabel => L("确认并继续", "Review and resume");
    public string CancelLabel => L("取消 Run", "Cancel run");

    [ObservableProperty] private bool _isSubmitting;
    [ObservableProperty] private string? _errorMessage;

    [RelayCommand] private Task PauseAsync() => ActAsync(CanPause,
        token => _service.PauseRunAsync(State.RunId, State.ConversationId, token));
    [RelayCommand] private Task ResumeAsync() => ActAsync(CanResume,
        token => _service.ResumeRunAsync(State.RunId, State.ConversationId, token));
    [RelayCommand] private Task CancelAsync() => ActAsync(CanCancel,
        token => _service.CancelRunAsync(State.RunId, State.ConversationId, token));

    private async Task ActAsync(bool allowed, Func<CancellationToken, Task> action)
    {
        if (!allowed || IsSubmitting) return;
        IsSubmitting = true;
        ErrorMessage = null;
        try
        {
            await action(CancellationToken.None);
            await _onChanged();
        }
        catch (Exception exception) { ErrorMessage = exception.Message; }
        finally { IsSubmitting = false; }
    }

    private string L(string chinese, string english) =>
        _localization?.Text(chinese, english) ?? chinese;
}
