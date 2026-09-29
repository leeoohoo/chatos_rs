using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IAuthenticationService
{
    Task<AuthSession?> RestoreSessionAsync(CancellationToken cancellationToken = default);

    Task<AuthSession> LoginAsync(
        string username,
        string password,
        CancellationToken cancellationToken = default);

    Task<RegistrationCodeDelivery> SendRegistrationCodeAsync(
        string email,
        string inviteCode,
        CancellationToken cancellationToken = default);

    Task<AuthSession> RegisterAsync(
        string email,
        string password,
        string inviteCode,
        string verificationCode,
        CancellationToken cancellationToken = default);

    ValueTask LogoutAsync(CancellationToken cancellationToken = default);
}
