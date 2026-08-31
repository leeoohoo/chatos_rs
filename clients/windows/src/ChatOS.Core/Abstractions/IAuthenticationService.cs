using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IAuthenticationService
{
    Task<AuthSession?> RestoreSessionAsync(CancellationToken cancellationToken = default);

    Task<AuthSession> LoginAsync(
        string username,
        string password,
        CancellationToken cancellationToken = default);

    ValueTask LogoutAsync(CancellationToken cancellationToken = default);
}
