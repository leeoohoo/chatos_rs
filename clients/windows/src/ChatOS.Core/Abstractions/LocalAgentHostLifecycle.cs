namespace ChatOS.Core.Abstractions;

public interface ILocalAgentHostLifecycle
{
    string? ActiveOwnerUserId { get; }

    Task StartForOwnerAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default);

    Task StopAsync(CancellationToken cancellationToken = default);
}
