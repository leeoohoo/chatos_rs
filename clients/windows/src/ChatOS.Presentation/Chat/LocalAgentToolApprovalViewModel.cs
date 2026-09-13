using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Chat;

public sealed partial class LocalAgentToolApprovalViewModel : ObservableObject
{
    private readonly ILocalAgentToolApprovalService _service;
    private readonly Func<Task> _onChanged;
    private readonly LocalizationViewModel? _localization;

    public LocalAgentToolApprovalViewModel(
        LocalAgentToolApprovalRequest request,
        ILocalAgentToolApprovalService service,
        Func<Task> onChanged,
        LocalizationViewModel? localization = null)
    {
        Request = request;
        _service = service;
        _onChanged = onChanged;
        _localization = localization;
    }

    public LocalAgentToolApprovalRequest Request { get; }
    public string Id => Request.InvocationId;
    public string ToolName => Request.ToolName;
    public string Title => L("等待工具授权", "Tool approval required");
    public string ApproveLabel => L("允许本次执行", "Allow this invocation");
    public string RejectLabel => L("拒绝本次执行", "Reject this invocation");
    public string EffectLabel => Request.Effect switch
    {
        LocalAgentToolEffect.Read => L("只读", "Read only"),
        LocalAgentToolEffect.IdempotentWrite => L("可重试写入", "Idempotent write"),
        LocalAgentToolEffect.Write => L("写入", "Write"),
        LocalAgentToolEffect.Billable => L("可能计费", "Billable"),
        LocalAgentToolEffect.Terminal => L("终止性操作", "Terminal"),
        _ => Request.Effect.ToString(),
    };
    public string Explanation => Request.Effect switch
    {
        LocalAgentToolEffect.Read => L(
            "这个请求只读取数据，授权仅适用于当前这一次调用。",
            "This request only reads data. Approval applies to this invocation only."),
        LocalAgentToolEffect.IdempotentWrite => L(
            "这个请求会修改数据，但使用稳定调用编号避免重复写入。",
            "This request changes data and uses a stable invocation ID to prevent duplicate writes."),
        LocalAgentToolEffect.Write => L(
            "这个请求会修改本地或外部数据，请确认工具与当前任务一致。",
            "This request changes local or external data. Confirm that it matches the current task."),
        LocalAgentToolEffect.Billable => L(
            "这个请求可能产生费用，授权仅适用于当前这一次调用。",
            "This request may incur a charge. Approval applies to this invocation only."),
        LocalAgentToolEffect.Terminal => L(
            "这个操作可能发布、提交或完成无法自动撤回的动作。",
            "This operation may publish or finalize an action that cannot be automatically undone."),
        _ => string.Empty,
    };
    public string DigestLabel => L(
        $"请求指纹：{ShortDigest(Request.ArgumentsDigest)}",
        $"Request fingerprint: {ShortDigest(Request.ArgumentsDigest)}");

    [ObservableProperty] private bool _isSubmitting;
    [ObservableProperty] private string? _errorMessage;

    [RelayCommand]
    private Task ApproveAsync() => DecideAsync(
        LocalAgentToolApprovalDecision.Approve,
        L("用户允许本次工具执行", "The user allowed this tool invocation"));

    [RelayCommand]
    private Task RejectAsync() => DecideAsync(
        LocalAgentToolApprovalDecision.Reject,
        L("用户拒绝本次工具执行", "The user rejected this tool invocation"));

    private async Task DecideAsync(LocalAgentToolApprovalDecision decision, string reason)
    {
        if (IsSubmitting) return;
        IsSubmitting = true;
        ErrorMessage = null;
        try
        {
            await _service.DecideAsync(
                Request.InvocationId,
                Request.ConversationId,
                decision,
                reason);
            await _onChanged();
        }
        catch (Exception exception)
        {
            ErrorMessage = exception.Message;
        }
        finally
        {
            IsSubmitting = false;
        }
    }

    private static string ShortDigest(string digest) => digest.Length <= 22
        ? digest
        : $"{digest[..15]}…{digest[^6..]}";

    private string L(string chinese, string english) =>
        _localization?.Text(chinese, english) ?? chinese;
}
