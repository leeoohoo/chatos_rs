namespace ChatOS.Connector.LocalAgent;

internal static class WindowsLocalAgentHostFailurePolicy
{
    internal static bool IsTransientEndpointLoss(Exception error) => error switch
    {
        WindowsLocalAgentAccountSessionException session =>
            session.Failure == WindowsLocalAgentAccountSessionFailure.HostUnavailable,
        EndOfStreamException => true,
        TimeoutException => true,
        IOException when error is not InvalidDataException => true,
        _ => false,
    };
}
