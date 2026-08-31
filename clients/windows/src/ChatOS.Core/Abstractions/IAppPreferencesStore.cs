using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IAppPreferencesStore
{
    Task<AppPreferences?> LoadAsync(CancellationToken cancellationToken = default);

    Task SaveAsync(AppPreferences preferences, CancellationToken cancellationToken = default);
}
