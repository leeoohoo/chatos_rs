namespace ChatOS.Connector.Approval;

public enum ApprovalReviewerReadinessState
{
    Ready,
    ModelNotSelected,
    ConnectorNotPaired,
    ManagedConfigurationInvalid,
}

public sealed record ApprovalReviewerReadiness(
    ApprovalReviewerReadinessState State,
    string? Detail = null)
{
    public bool IsReady => State is ApprovalReviewerReadinessState.Ready;
}

public interface IApprovalReviewerReadinessService
{
    Task<ApprovalReviewerReadiness> CheckAsync(CancellationToken cancellationToken = default);
}

internal sealed class ApprovalReviewerReadinessService(
    ApprovalModelRuntimeConfigurationService configuration) : IApprovalReviewerReadinessService
{
    public async Task<ApprovalReviewerReadiness> CheckAsync(
        CancellationToken cancellationToken = default)
    {
        try
        {
            await configuration.ResolveAsync(cancellationToken).ConfigureAwait(false);
            return new ApprovalReviewerReadiness(ApprovalReviewerReadinessState.Ready);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (InvalidOperationException exception) when (
            exception.Message.Contains("not selected", StringComparison.OrdinalIgnoreCase))
        {
            return new ApprovalReviewerReadiness(
                ApprovalReviewerReadinessState.ModelNotSelected,
                exception.Message);
        }
        catch (InvalidOperationException exception) when (
            exception.Message.Contains("not paired", StringComparison.OrdinalIgnoreCase) ||
            exception.Message.Contains("owner is unavailable", StringComparison.OrdinalIgnoreCase))
        {
            return new ApprovalReviewerReadiness(
                ApprovalReviewerReadinessState.ConnectorNotPaired,
                exception.Message);
        }
        catch (Exception exception)
        {
            return new ApprovalReviewerReadiness(
                ApprovalReviewerReadinessState.ManagedConfigurationInvalid,
                SafeDetail(exception.Message));
        }
    }

    private static string SafeDetail(string value)
    {
        if (string.IsNullOrWhiteSpace(value) || value.Any(char.IsControl))
        {
            return "The managed approval configuration is unavailable.";
        }

        return value[..Math.Min(300, value.Length)];
    }
}
