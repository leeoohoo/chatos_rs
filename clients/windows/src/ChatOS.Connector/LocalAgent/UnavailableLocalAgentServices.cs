using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;

namespace ChatOS.Connector.LocalAgent;

internal sealed class UnavailableLocalAgentHostClient : ILocalAgentHostClient
{
    public string? ActiveOwnerUserId => null;

    public Task StartForOwnerAsync(string ownerUserId, CancellationToken cancellationToken = default) =>
        throw Unavailable();

    public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

    public Task RestartForOwnerAsync(
        string ownerUserId,
        IReadOnlyDictionary<string, string> credentialEnvironment,
        CancellationToken cancellationToken = default) =>
        throw Unavailable();

    public Task<TResponse> SendAsync<TCommand, TResponse>(
        TCommand command,
        CancellationToken cancellationToken = default)
        where TCommand : notnull => throw Unavailable();

    private static InvalidOperationException Unavailable() =>
        new("The optional local agent host is not installed.");
}

internal sealed class UnavailableWorkspaceRelationsService : IWorkspaceRelationsService
{
    public Task<WorkspaceRelationsSnapshot> FetchWorkspaceRelationsAsync(
        CancellationToken cancellationToken = default) =>
        Task.FromResult(new WorkspaceRelationsSnapshot([], []));
}

internal sealed class UnavailableProjectConversationService : IProjectConversationService
{
    public Task<string> EnsureConversationAsync(
        WorkspaceProject project,
        WorkspaceContact contact,
        CancellationToken cancellationToken = default) =>
        throw new InvalidOperationException(
            "Project conversations require the optional local agent host.");
}
