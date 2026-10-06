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
    bool PetEnabled,
    double PetSize = 104,
    bool PetShowProcess = true,
    bool PetShowCompletions = true)
{
    public const double MinimumFontScale = 0.85;
    public const double MaximumFontScale = 1.30;
    public const double MinimumPetSize = 72;
    public const double MaximumPetSize = 180;
    public const double DefaultPetSize = 104;

    public static AppPreferences Default { get; } = new(
        InterfaceLanguage.SimplifiedChinese,
        InterfaceTheme.System,
        1.0,
        true,
        DefaultPetSize);

    public AppPreferences Normalize() => this with
    {
        FontScale = Math.Clamp(FontScale, MinimumFontScale, MaximumFontScale),
        PetSize = double.IsFinite(PetSize)
            ? Math.Clamp(PetSize, MinimumPetSize, MaximumPetSize)
            : DefaultPetSize,
    };
}
