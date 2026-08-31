namespace ChatOS.Core.Domain;

public enum InterfaceLanguage
{
    SimplifiedChinese,
    English,
}

public enum InterfaceTheme
{
    System,
    Light,
    Dark,
}

public sealed record AppPreferences(
    InterfaceLanguage Language,
    InterfaceTheme Theme,
    double FontScale,
    bool PetEnabled)
{
    public const double MinimumFontScale = 0.85;
    public const double MaximumFontScale = 1.30;

    public static AppPreferences Default { get; } = new(
        InterfaceLanguage.SimplifiedChinese,
        InterfaceTheme.System,
        1.0,
        true);

    public AppPreferences Normalize() => this with
    {
        FontScale = Math.Clamp(FontScale, MinimumFontScale, MaximumFontScale),
    };
}
