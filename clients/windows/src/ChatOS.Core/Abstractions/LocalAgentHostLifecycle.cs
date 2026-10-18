namespace ChatOS.Core.Abstractions;

public interface ILocalAgentHostLifecycle
{
    string? ActiveOwnerUserId { get; }

    Task StartForOwnerAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default);

    Task StopAsync(CancellationToken cancellationToken = default);
}

public interface ILocalAgentHostClient : ILocalAgentHostLifecycle
{
    Task StartForBootstrapAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default) =>
        StartForOwnerAsync(ownerUserId, cancellationToken);

    Task RestartForOwnerAsync(
        string ownerUserId,
        IReadOnlyDictionary<string, string> credentialEnvironment,
        CancellationToken cancellationToken = default);

    Task RefreshCredentialsForOwnerAsync(
        string ownerUserId,
        IReadOnlyDictionary<string, string> credentialEnvironment,
        CancellationToken cancellationToken = default) =>
        RestartForOwnerAsync(ownerUserId, credentialEnvironment, cancellationToken);

    Task<TResponse> SendAsync<TCommand, TResponse>(
        TCommand command,
        CancellationToken cancellationToken = default)
        where TCommand : notnull;
}
