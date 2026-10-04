using System.Text.Json;

namespace ChatOS.Connector.Relay;

internal static class CanonicalJson
{
    public static string Serialize(JsonElement value)
    {
        try
        {
            return NetworkGuard.Contracts.CanonicalJson.Serialize(value);
        }
        catch (InvalidDataException exception)
        {
            throw new RelayRequestException(400, exception.Message);
        }
    }
}
