namespace ChatOS.Core.Abstractions;

public interface IPetFavoriteProjectsStore
{
    Task<IReadOnlyList<string>> LoadAsync(CancellationToken cancellationToken = default);

    Task SaveAsync(
        IReadOnlyCollection<string> projectIds,
        CancellationToken cancellationToken = default);
}
