using System.Reflection;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace ChatOS.Connector.LocalAgent;

/// <summary>
/// Windows projection of the model-specific reasoning contract shared by macOS
/// and the Rust request builder. The catalog is embedded in ChatOS.Connector so
/// packaged builds never depend on a source-tree path at runtime.
/// </summary>
internal sealed record WindowsModelReasoningCapabilities(
    [property: JsonPropertyName("model")] string Model,
    [property: JsonPropertyName("levels")] IReadOnlyList<string> Levels,
    [property: JsonPropertyName("defaultLevel")] string DefaultLevel,
    [property: JsonPropertyName("legacyAliases")] IReadOnlyDictionary<string, string> LegacyAliases)
{
    private sealed record Catalog(
        [property: JsonPropertyName("models")] IReadOnlyList<WindowsModelReasoningCapabilities> Models);

    private const string ResourceSuffix = "ModelReasoningCapabilities.json";

    private static readonly IReadOnlyList<WindowsModelReasoningCapabilities> All = Load();

    public static WindowsModelReasoningCapabilities? ForModel(string model)
    {
        var normalized = model.Trim().ToLowerInvariant();
        return All.FirstOrDefault(capability =>
            normalized == capability.Model || IsDatedVariant(normalized, capability.Model));
    }

    public string? Normalize(string? value)
    {
        var normalized = value?.Trim().ToLowerInvariant();
        if (string.IsNullOrEmpty(normalized) || normalized == "auto") return null;
        if (Levels.Contains(normalized, StringComparer.Ordinal)) return normalized;
        return LegacyAliases.GetValueOrDefault(normalized);
    }

    private static bool IsDatedVariant(string value, string model)
    {
        if (!value.StartsWith(model + "-", StringComparison.Ordinal)) return false;
        var suffix = value[(model.Length + 1)..];
        return suffix.Length == 10 && suffix.All(character =>
            char.IsAsciiDigit(character) || character == '-');
    }

    private static IReadOnlyList<WindowsModelReasoningCapabilities> Load()
    {
        var assembly = typeof(WindowsModelReasoningCapabilities).Assembly;
        var resource = assembly.GetManifestResourceNames().Single(name =>
            name.EndsWith(ResourceSuffix, StringComparison.Ordinal));
        using var stream = assembly.GetManifestResourceStream(resource)
            ?? throw new InvalidOperationException("Missing embedded model reasoning capabilities.");
        var catalog = JsonSerializer.Deserialize<Catalog>(stream)
            ?? throw new InvalidDataException("Invalid embedded model reasoning capabilities.");
        if (catalog.Models.Count == 0 || catalog.Models.Any(capability =>
                string.IsNullOrWhiteSpace(capability.Model) ||
                capability.Levels.Count == 0 ||
                !capability.Levels.Contains(capability.DefaultLevel, StringComparer.Ordinal)))
        {
            throw new InvalidDataException("Invalid embedded model reasoning capability entries.");
        }
        return catalog.Models;
    }
}
