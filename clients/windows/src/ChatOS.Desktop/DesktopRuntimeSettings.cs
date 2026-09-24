using System.Text.Json;
using System.Text.Json.Serialization;

namespace ChatOS.Desktop;

internal sealed record DesktopRuntimeSettings(
    [property: JsonPropertyName("api_base_url")] string? ApiBaseUrl,
    [property: JsonPropertyName("local_connector_cloud_base_url")] string? LocalConnectorCloudBaseUrl)
{
    public static DesktopRuntimeSettings Load()
    {
        var path = Path.Combine(AppContext.BaseDirectory, "chatos.runtime.json");
        if (!File.Exists(path)) return new(null, null);

        using var stream = File.OpenRead(path);
        return JsonSerializer.Deserialize<DesktopRuntimeSettings>(stream)
            ?? throw new InvalidDataException($"Runtime settings are empty: {path}");
    }
}
