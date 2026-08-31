using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IProjectCodeNavigationService
{
    Task<ProjectCodeNavigationResult> DefinitionAsync(
        string projectRoot,
        string filePath,
        int line,
        int column,
        CancellationToken cancellationToken = default);

    Task<ProjectCodeNavigationResult> ReferencesAsync(
        string projectRoot,
        string filePath,
        int line,
        int column,
        CancellationToken cancellationToken = default);
}
