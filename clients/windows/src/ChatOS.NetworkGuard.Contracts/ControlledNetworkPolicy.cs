using System.Globalization;
using System.Net;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using Org.BouncyCastle.Crypto.Parameters;
using Org.BouncyCastle.Math.EC.Rfc8032;

namespace ChatOS.NetworkGuard.Contracts;

public sealed record ControlledNetworkPolicyEnvelope(
    [property: JsonPropertyName("policy_revision")] string PolicyRevision,
    [property: JsonPropertyName("owner_user_id")] string OwnerUserId,
    [property: JsonPropertyName("device_id")] string DeviceId,
    [property: JsonPropertyName("workspace_id")] string WorkspaceId,
    [property: JsonPropertyName("windows_user_sid")] string WindowsUserSid,
    [property: JsonPropertyName("allowed_hosts")] IReadOnlyList<string> AllowedHosts,
    [property: JsonPropertyName("allowed_ports")] IReadOnlyList<int>? AllowedPorts,
    [property: JsonPropertyName("expires_at")] DateTimeOffset ExpiresAt,
    [property: JsonPropertyName("signature_key_id")] string SignatureKeyId,
    [property: JsonPropertyName("signature_alg")] string SignatureAlgorithm,
    [property: JsonPropertyName("signature")] string Signature);

public sealed record ControlledNetworkPolicy(
    string PolicyRevision,
    string OwnerUserId,
    string DeviceId,
    string WorkspaceId,
    string WindowsUserSid,
    IReadOnlyList<string> AllowedHosts,
    IReadOnlyList<int> AllowedPorts,
    DateTimeOffset ExpiresAt,
    string SignatureKeyId)
{
    public bool Allows(string host, int port)
    {
        if (!AllowedPorts.Contains(port)) return false;
        string normalized;
        try
        {
            normalized = ControlledNetworkPolicyValidator.NormalizeHost(host, allowWildcard: false);
        }
        catch (ArgumentException)
        {
            return false;
        }

        return AllowedHosts.Any(rule => RuleMatches(rule, normalized));
    }

    private static bool RuleMatches(string rule, string host)
    {
        if (!rule.StartsWith("*.", StringComparison.Ordinal))
        {
            return string.Equals(rule, host, StringComparison.Ordinal);
        }

        var suffix = rule[2..];
        if (!host.EndsWith('.' + suffix, StringComparison.Ordinal)) return false;
        var prefixLength = host.Length - suffix.Length - 1;
        return prefixLength > 0 && !host.AsSpan(0, prefixLength).Contains('.');
    }
}

public sealed class ControlledNetworkPolicyValidator
{
    internal const int MaximumHostCount = 256;
    internal static readonly TimeSpan MaximumLifetime = TimeSpan.FromHours(24);
    private static readonly int[] DefaultPorts = [80, 443];
    private readonly IReadOnlyDictionary<string, string> _trustedPublicKeys;
    private readonly TimeProvider _timeProvider;

    public ControlledNetworkPolicyValidator(
        IReadOnlyDictionary<string, string> trustedPublicKeys,
        TimeProvider? timeProvider = null)
    {
        _trustedPublicKeys = trustedPublicKeys;
        _timeProvider = timeProvider ?? TimeProvider.System;
    }

    public ControlledNetworkPolicy Validate(ControlledNetworkPolicyEnvelope envelope)
    {
        ArgumentNullException.ThrowIfNull(envelope);
        var revision = Required(envelope.PolicyRevision, "policy revision", 128);
        var owner = Required(envelope.OwnerUserId, "owner user id", 256);
        var device = Required(envelope.DeviceId, "device id", 256);
        var workspace = Required(envelope.WorkspaceId, "workspace id", 256);
        var windowsUserSid = RequiredWindowsUserSid(envelope.WindowsUserSid);
        var keyId = Required(envelope.SignatureKeyId, "signature key id", 128);
        if (!string.Equals(envelope.SignatureAlgorithm?.Trim(), "ed25519", StringComparison.Ordinal))
        {
            throw new InvalidOperationException("Controlled network policy signature algorithm is unsupported.");
        }
        if (!_trustedPublicKeys.TryGetValue(keyId, out var publicKeyText))
        {
            throw new InvalidOperationException("Controlled network policy signing key is not trusted.");
        }

        var now = _timeProvider.GetUtcNow();
        if (envelope.ExpiresAt <= now || envelope.ExpiresAt > now + MaximumLifetime)
        {
            throw new InvalidOperationException("Controlled network policy expiry is invalid.");
        }
        if (envelope.AllowedHosts is null || envelope.AllowedHosts.Count is < 1 or > MaximumHostCount)
        {
            throw new InvalidOperationException("Controlled network policy host count is invalid.");
        }

        var hosts = envelope.AllowedHosts
            .Select(value => NormalizeHost(value, allowWildcard: true))
            .Distinct(StringComparer.Ordinal)
            .OrderBy(value => value, StringComparer.Ordinal)
            .ToArray();
        var ports = (envelope.AllowedPorts is null or { Count: 0 }
                ? DefaultPorts
                : envelope.AllowedPorts)
            .Distinct()
            .Order()
            .ToArray();
        if (ports.Length is < 1 or > 2 || ports.Any(port => port is not (80 or 443)))
        {
            throw new InvalidOperationException("Controlled network policy only supports HTTP and HTTPS ports.");
        }

        var normalized = new ControlledNetworkPolicy(
            revision,
            owner,
            device,
            workspace,
            windowsUserSid,
            hosts,
            ports,
            envelope.ExpiresAt.ToUniversalTime(),
            keyId);
        VerifySignature(normalized, envelope.Signature, publicKeyText);
        return normalized;
    }

    internal static byte[] SignaturePayload(ControlledNetworkPolicy policy)
    {
        var value = JsonSerializer.SerializeToElement(new
        {
            policy_revision = policy.PolicyRevision,
            owner_user_id = policy.OwnerUserId,
            device_id = policy.DeviceId,
            workspace_id = policy.WorkspaceId,
            windows_user_sid = policy.WindowsUserSid,
            allowed_hosts = policy.AllowedHosts,
            allowed_ports = policy.AllowedPorts,
            expires_at = policy.ExpiresAt.ToUnixTimeSeconds(),
            signature_key_id = policy.SignatureKeyId,
            signature_alg = "ed25519",
        });
        return Encoding.UTF8.GetBytes(CanonicalJson.Serialize(value));
    }

    internal static string NormalizeHost(string value, bool allowWildcard)
    {
        var host = value?.Trim()
            ?? throw new ArgumentException("Controlled network host is required.", nameof(value));
        if (host.EndsWith(".", StringComparison.Ordinal)) host = host[..^1];
        var wildcard = allowWildcard && host.StartsWith("*.", StringComparison.Ordinal);
        if (wildcard) host = host[2..];
        if (host.Length == 0 || host.Contains('*') || host.Contains('/') || host.Contains(':') ||
            host.Contains('@') || host.Any(char.IsWhiteSpace) || host.EndsWith(".", StringComparison.Ordinal))
        {
            throw new ArgumentException("Controlled network host is invalid.", nameof(value));
        }

        string ascii;
        try
        {
            ascii = new IdnMapping().GetAscii(host).ToLowerInvariant();
        }
        catch (ArgumentException)
        {
            throw new ArgumentException("Controlled network host is invalid.", nameof(value));
        }

        if (ascii.Length > 253)
        {
            throw new ArgumentException("Controlled network host is too long.", nameof(value));
        }
        if (IPAddress.TryParse(ascii, out _))
        {
            throw new ArgumentException("Controlled network IP literals require a separate explicit policy.", nameof(value));
        }
        var labels = ascii.Split('.');
        if (labels.Length < 2 || labels.Any(label => label.Length is < 1 or > 63 ||
            label[0] == '-' || label[^1] == '-' ||
            label.Any(character => !char.IsAsciiLetterOrDigit(character) && character != '-')))
        {
            throw new ArgumentException("Controlled network host is invalid.", nameof(value));
        }

        return wildcard ? "*." + ascii : ascii;
    }

    private static void VerifySignature(
        ControlledNetworkPolicy policy,
        string signatureText,
        string publicKeyText)
    {
        var signature = DecodeBase64Url(Required(signatureText, "signature", 512));
        var publicKey = DecodeBase64Url(RemovePrefix(publicKeyText.Trim(), "ed25519:"));
        bool valid;
        try
        {
            valid = new Ed25519PublicKeyParameters(publicKey).Verify(
                Ed25519.Algorithm.Ed25519,
                null,
                SignaturePayload(policy),
                signature);
        }
        catch (ArgumentException)
        {
            valid = false;
        }
        if (!valid)
        {
            throw new InvalidOperationException("Controlled network policy signature verification failed.");
        }
    }

    private static string Required(string? value, string label, int maximumLength)
    {
        var result = value?.Trim();
        if (string.IsNullOrWhiteSpace(result) || result.Length > maximumLength || result.Any(char.IsControl))
        {
            throw new InvalidOperationException($"Controlled network policy {label} is invalid.");
        }
        return result;
    }

    private static string RequiredWindowsUserSid(string? value)
    {
        var result = Required(value, "Windows user SID", 184);
        if (!result.StartsWith("S-1-", StringComparison.Ordinal) ||
            result.Split('-').Skip(1).Any(part => part.Length == 0 || part.Any(character => !char.IsAsciiDigit(character))))
        {
            throw new InvalidOperationException("Controlled network policy Windows user SID is invalid.");
        }
        return result;
    }

    private static byte[] DecodeBase64Url(string value)
    {
        try
        {
            var normalized = value.Replace('-', '+').Replace('_', '/');
            normalized += new string('=', (4 - normalized.Length % 4) % 4);
            return Convert.FromBase64String(normalized);
        }
        catch (FormatException)
        {
            throw new InvalidOperationException("Controlled network policy signature encoding is invalid.");
        }
    }

    private static string RemovePrefix(string value, string prefix) =>
        value.StartsWith(prefix, StringComparison.Ordinal) ? value[prefix.Length..] : value;
}
