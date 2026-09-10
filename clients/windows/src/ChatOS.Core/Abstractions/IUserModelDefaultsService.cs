namespace ChatOS.Core.Abstractions;

public sealed record UserModelDefaults(string? TaskRunnerDefaultModelConfigId);

public interface IUserModelDefaultsService
{
    Task<UserModelDefaults> FetchAsync(CancellationToken cancellationToken = default);

    Task<UserModelDefaults> UpdateTaskRunnerDefaultAsync(
        string? defaultModelConfigId,
        CancellationToken cancellationToken = default);
}
