using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IProjectRunService
{
    Task<ProjectRunCatalog> FetchCatalogAsync(
        string projectId,
        CancellationToken cancellationToken = default);

    Task<ProjectRunCatalog> AnalyzeAsync(
        string projectId,
        CancellationToken cancellationToken = default);

    Task<ProjectRunState> FetchStateAsync(
        string projectId,
        CancellationToken cancellationToken = default);

    Task<ProjectRunEnvironment> FetchEnvironmentAsync(
        string projectId,
        CancellationToken cancellationToken = default);

    Task<ProjectRunEnvironment> UpdateEnvironmentAsync(
        string projectId,
        IReadOnlyDictionary<string, string> selectedToolchains,
        IReadOnlyDictionary<string, ProjectRunCustomToolchain> customToolchains,
        IReadOnlyDictionary<string, string> environmentVariables,
        CancellationToken cancellationToken = default);

    Task<ProjectRunCatalog> SetDefaultTargetAsync(
        string projectId,
        string targetId,
        CancellationToken cancellationToken = default);

    Task StartAsync(
        string projectId,
        string targetId,
        CancellationToken cancellationToken = default);

    Task StopAsync(
        string instanceId,
        CancellationToken cancellationToken = default);

    Task DeleteAsync(
        string instanceId,
        CancellationToken cancellationToken = default);
}
