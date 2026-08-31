using Org.BouncyCastle.Crypto.Parameters;
using Org.BouncyCastle.Math.EC.Rfc8032;

namespace ChatOS.Connector.Relay;

public sealed class Ed25519RelayRequestVerifier : IRelayRequestVerifier
{
    private readonly object _nonceGate = new();
    private readonly Dictionary<string, long> _seenNonces = new(StringComparer.Ordinal);
    private readonly IRelaySecurityContextProvider _contextProvider;
    private readonly TimeProvider _timeProvider;

    public Ed25519RelayRequestVerifier(
        IRelaySecurityContextProvider contextProvider,
        TimeProvider? timeProvider = null)
    {
        _contextProvider = contextProvider;
        _timeProvider = timeProvider ?? TimeProvider.System;
    }

    public async Task VerifyAsync(
        RelayRequest request,
        CancellationToken cancellationToken)
    {
        var context = await _contextProvider.GetAsync(cancellationToken).ConfigureAwait(false);
        if (!string.Equals(request.OwnerUserId, context.OwnerUserId, StringComparison.Ordinal))
        {
            throw new RelayRequestException(403, "Relay owner does not match the paired account.");
        }

        if (!string.Equals(request.DeviceId, context.DeviceId, StringComparison.Ordinal))
        {
            throw new RelayRequestException(403, "Relay device does not match this connector.");
        }

        var hasSignature = request.PlatformSignature is not null ||
            request.PlatformSignatureKeyId is not null ||
            request.PlatformSignatureAlgorithm is not null ||
            request.PlatformTimestamp is not null ||
            request.PlatformNonce is not null;
        if (!hasSignature)
        {
            if (context.Trust.RequireSignedMessages)
            {
                throw new RelayRequestException(403, "Relay platform signature is required.");
            }

            return;
        }

        if (!string.Equals(
                request.PlatformSignatureAlgorithm?.Trim(),
                "ed25519",
                StringComparison.Ordinal))
        {
            throw new RelayRequestException(403, "Relay platform signature algorithm is unsupported.");
        }

        var keyId = Required(request.PlatformSignatureKeyId, "platform signing key id");
        if (!context.Trust.TrustedRelayPublicKeys.TryGetValue(keyId, out var publicKeyText))
        {
            throw new RelayRequestException(403, "Relay platform signing key is not trusted.");
        }

        var timestamp = request.PlatformTimestamp
            ?? throw new RelayRequestException(403, "Relay platform timestamp is required.");
        var now = _timeProvider.GetUtcNow().ToUnixTimeSeconds();
        var maximumSkew = Math.Clamp(context.Trust.SignatureMaxSkewSeconds, 1, 3_600);
        var skew = SaturatingSubtract(now, timestamp);
        if (skew > maximumSkew || skew < -maximumSkew)
        {
            throw new RelayRequestException(403, "Relay platform signature has expired.");
        }

        var nonce = Required(request.PlatformNonce, "platform nonce");
        if (nonce.Length is < 16 or > 128)
        {
            throw new RelayRequestException(403, "Relay platform nonce is invalid.");
        }

        var signature = DecodeBase64Url(Required(request.PlatformSignature, "platform signature"));
        var publicKey = DecodeBase64Url(RemovePrefix(publicKeyText.Trim(), "ed25519:"));
        bool valid;
        try
        {
            valid = new Ed25519PublicKeyParameters(publicKey)
                .Verify(Ed25519.Algorithm.Ed25519, null, RelayRequestSignature.Payload(request), signature);
        }
        catch (ArgumentException)
        {
            valid = false;
        }

        if (!valid)
        {
            throw new RelayRequestException(403, "Relay platform signature verification failed.");
        }

        ConsumeNonce(keyId, nonce, now, maximumSkew);
    }

    private void ConsumeNonce(string keyId, string nonce, long now, int maximumSkew)
    {
        var cacheKey = $"{keyId}:{nonce}";
        lock (_nonceGate)
        {
            var minimumExpiry = now - maximumSkew;
            foreach (var expired in _seenNonces
                .Where(pair => pair.Value < minimumExpiry)
                .Select(pair => pair.Key)
                .ToArray())
            {
                _seenNonces.Remove(expired);
            }

            if (_seenNonces.ContainsKey(cacheKey))
            {
                throw new RelayRequestException(403, "Relay platform nonce was already used.");
            }

            _seenNonces[cacheKey] = now + maximumSkew;
        }
    }

    private static byte[] DecodeBase64Url(string value)
    {
        try
        {
            var normalized = value.Replace('-', '+').Replace('_', '/');
            normalized += new string('=', (4 - normalized.Length % 4) % 4);
            return Convert.FromBase64String(normalized);
        }
        catch (FormatException exception)
        {
            throw new RelayRequestException(403, $"Relay signature encoding is invalid: {exception.Message}");
        }
    }

    private static string Required(string? value, string label) =>
        !string.IsNullOrWhiteSpace(value)
            ? value.Trim()
            : throw new RelayRequestException(403, $"Relay {label} is required.");

    private static string RemovePrefix(string value, string prefix) =>
        value.StartsWith(prefix, StringComparison.Ordinal)
            ? value[prefix.Length..]
            : value;

    private static long SaturatingSubtract(long left, long right)
    {
        try
        {
            return checked(left - right);
        }
        catch (OverflowException)
        {
            return left >= right ? long.MaxValue : long.MinValue;
        }
    }
}
