using ChatOS.Presentation.Tasks;

namespace ChatOS.Presentation.Chat;

public sealed partial class ConversationReplyItemViewModel
{
    private CancellationTokenSource? _inspectorCancellation;
    private long _inspectorGeneration;

    public async Task ToggleInspectorAsync(
        TaskReplyInspectorSection section,
        Func<MessageTaskGraphRequest, CancellationToken, Task<TaskReplyInspectorSnapshot>> load)
    {
        if (TaskGraphRequest is not { } request) return;
        if (!BeginInspector(section)) return;

        using var cancellation = new CancellationTokenSource();
        _inspectorCancellation = cancellation;
        var generation = _inspectorGeneration;
        try
        {
            // Preserve the caller's UI context, as the desktop click handler did.
            var snapshot = await load(request, cancellation.Token);
            if (IsCurrent()) CompleteInspector(snapshot.Task, snapshot.Run);
        }
        catch (OperationCanceledException) when (cancellation.IsCancellationRequested)
        {
            // Collapse, replacement and leaving the conversation are normal.
        }
        catch (Exception exception)
        {
            if (IsCurrent()) FailInspector(exception.Message);
        }
        finally
        {
            if (ReferenceEquals(_inspectorCancellation, cancellation))
                _inspectorCancellation = null;
        }

        bool IsCurrent() => !cancellation.IsCancellationRequested &&
            generation == _inspectorGeneration && InspectorSection == section;
    }

    public void CloseInspector()
    {
        CancelInspectorLoad();
        InspectorSection = TaskReplyInspectorSection.None;
        IsInspectorLoading = false;
        InspectorTask = null;
        InspectorRun = null;
        InspectorError = null;
    }

    private void CancelInspectorLoad()
    {
        _inspectorGeneration++;
        var previous = _inspectorCancellation;
        _inspectorCancellation = null;
        // The async invocation owns disposal after its continuation has ended.
        previous?.Cancel();
    }
}
