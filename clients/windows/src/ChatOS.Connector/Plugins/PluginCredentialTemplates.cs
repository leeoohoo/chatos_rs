namespace ChatOS.Connector.Plugins;

internal sealed record PluginCredentialTemplate(
    string Prefix,
    string? SecretName,
    string Suffix)
{
    private const string PlaceholderPrefix = "${credential:";

    public static PluginCredentialTemplate Parse(string value)
    {
        if (string.IsNullOrEmpty(value) || value.Length > 8 * 1024 || value.Any(char.IsControl))
        {
            throw new PluginRuntimeException("Plugin credential template is empty, oversized, or contains controls.");
        }

        var start = value.IndexOf(PlaceholderPrefix, StringComparison.Ordinal);
        if (start < 0)
        {
            return new PluginCredentialTemplate(value, null, string.Empty);
        }

        var nameStart = start + PlaceholderPrefix.Length;
        var end = value.IndexOf('}', nameStart);
        if (end < 0 || value.IndexOf(PlaceholderPrefix, nameStart, StringComparison.Ordinal) >= 0)
        {
            throw new PluginRuntimeException("Plugin credential template must contain exactly one placeholder.");
        }

        var name = value[nameStart..end];
        _ = new PluginCredentialScope("owner", "device", "plugin", "release", "component", name);
        return new PluginCredentialTemplate(value[..start], name, value[(end + 1)..]);
    }

    public async Task<string> ResolveAsync(
        PluginCredentialVault credentials,
        PluginCredentialScope scope,
        CancellationToken cancellationToken)
    {
        if (SecretName is null)
        {
            return Prefix;
        }

        var secret = await credentials.ResolveAsync(scope, cancellationToken).ConfigureAwait(false);
        return Prefix + secret + Suffix;
    }
}
