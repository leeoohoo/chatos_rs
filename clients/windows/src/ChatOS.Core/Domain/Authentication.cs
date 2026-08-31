namespace ChatOS.Core.Domain;

public sealed record AuthUser(
    string Id,
    string Username,
    string? DisplayName,
    string Role)
{
    public string EffectiveDisplayName =>
        string.IsNullOrWhiteSpace(DisplayName) ? Username : DisplayName.Trim();
}

public sealed record AuthSession(AuthUser User);
