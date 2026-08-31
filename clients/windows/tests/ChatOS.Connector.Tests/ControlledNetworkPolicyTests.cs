using ChatOS.NetworkGuard.Contracts;
using Org.BouncyCastle.Crypto.Parameters;
using Org.BouncyCastle.Math.EC.Rfc8032;
using Org.BouncyCastle.Security;

namespace ChatOS.Connector.Tests;

public sealed class ControlledNetworkPolicyTests
{
    [Fact]
    public void SignedPolicyNormalizesAndMatchesExactAndSingleLabelWildcard()
    {
        var context = new PolicyContext();
        var envelope = context.Sign(["API.Example.com.", "*.Example.com", "例子.测试"], null);

        var policy = context.Validator.Validate(envelope);

        Assert.Equal(["*.example.com", "api.example.com", "xn--fsqu00a.xn--0zwm56d"], policy.AllowedHosts);
        Assert.Equal([80, 443], policy.AllowedPorts);
        Assert.True(policy.Allows("api.example.com", 443));
        Assert.True(policy.Allows("www.example.com", 80));
        Assert.False(policy.Allows("example.com", 443));
        Assert.False(policy.Allows("a.b.example.com", 443));
        Assert.False(policy.Allows("www.example.com", 22));
    }

    [Theory]
    [InlineData("localhost")]
    [InlineData("https://example.com")]
    [InlineData("*.*.example.com")]
    [InlineData("exa_mple.com")]
    [InlineData("-bad.example.com")]
    [InlineData("bad-.example.com")]
    [InlineData("127.0.0.1")]
    [InlineData("[::1]")]
    [InlineData("example.com..")]
    [InlineData("exa mple.com")]
    public void InvalidHostIsRejected(string host)
    {
        var context = new PolicyContext();
        Assert.ThrowsAny<Exception>(() => context.Validator.Validate(context.Sign([host], [443])));
    }

    [Fact]
    public void UnsupportedPortIsRejected()
    {
        var context = new PolicyContext();
        Assert.Throws<InvalidOperationException>(() =>
            context.Validator.Validate(context.Sign(["example.com"], [22])));
    }

    [Fact]
    public void ExpiredAndOverlongPoliciesAreRejected()
    {
        var context = new PolicyContext();
        Assert.Throws<InvalidOperationException>(() => context.Validator.Validate(
            context.Sign(["example.com"], [443], context.Now.AddSeconds(-1))));
        Assert.Throws<InvalidOperationException>(() => context.Validator.Validate(
            context.Sign(["example.com"], [443], context.Now.AddHours(25))));
    }

    [Fact]
    public void TamperingAfterSigningIsRejected()
    {
        var context = new PolicyContext();
        var signed = context.Sign(["api.example.com"], [443]);
        Assert.Throws<InvalidOperationException>(() => context.Validator.Validate(
            signed with { AllowedHosts = ["evil.example.com"] }));
    }

    [Fact]
    public void UnknownKeyAndAlgorithmAreRejected()
    {
        var context = new PolicyContext();
        var signed = context.Sign(["api.example.com"], [443]);
        Assert.Throws<InvalidOperationException>(() => context.Validator.Validate(
            signed with { SignatureKeyId = "unknown" }));
        Assert.Throws<InvalidOperationException>(() => context.Validator.Validate(
            signed with { SignatureAlgorithm = "rsa" }));
    }

    private sealed class PolicyContext
    {
        private readonly Ed25519PrivateKeyParameters _privateKey = new(new SecureRandom());
        private readonly FrozenTimeProvider _time = new(DateTimeOffset.Parse("2026-08-30T12:00:00Z"));

        public PolicyContext()
        {
            var publicKey = Base64Url(_privateKey.GeneratePublicKey().GetEncoded());
            Validator = new ControlledNetworkPolicyValidator(
                new Dictionary<string, string> { ["network-key-1"] = "ed25519:" + publicKey },
                _time);
        }

        public DateTimeOffset Now => _time.GetUtcNow();

        public ControlledNetworkPolicyValidator Validator { get; }

        public ControlledNetworkPolicyEnvelope Sign(
            IReadOnlyList<string> hosts,
            IReadOnlyList<int>? ports,
            DateTimeOffset? expiresAt = null)
        {
            var expiry = expiresAt ?? Now.AddHours(1);
            var normalizedHosts = hosts
                .Select(value => ControlledNetworkPolicyValidator.NormalizeHost(value, allowWildcard: true))
                .Distinct(StringComparer.Ordinal)
                .OrderBy(value => value, StringComparer.Ordinal)
                .ToArray();
            var normalizedPorts = (ports is null or { Count: 0 } ? new[] { 80, 443 } : ports)
                .Distinct().Order().ToArray();
            var policy = new ControlledNetworkPolicy(
                "policy-1",
                "owner-1",
                "device-1",
                "workspace-1",
                "S-1-5-21-100-200-300-400",
                normalizedHosts,
                normalizedPorts,
                expiry.ToUniversalTime(),
                "network-key-1");
            var signature = new byte[Ed25519PrivateKeyParameters.SignatureSize];
            _privateKey.Sign(
                Ed25519.Algorithm.Ed25519,
                null,
                ControlledNetworkPolicyValidator.SignaturePayload(policy),
                signature);
            return new ControlledNetworkPolicyEnvelope(
                policy.PolicyRevision,
                policy.OwnerUserId,
                policy.DeviceId,
                policy.WorkspaceId,
                policy.WindowsUserSid,
                hosts,
                ports,
                policy.ExpiresAt,
                policy.SignatureKeyId,
                "ed25519",
                Base64Url(signature));
        }

        private static string Base64Url(byte[] value) =>
            Convert.ToBase64String(value).TrimEnd('=').Replace('+', '-').Replace('/', '_');
    }

    private sealed class FrozenTimeProvider(DateTimeOffset value) : TimeProvider
    {
        public override DateTimeOffset GetUtcNow() => value;
    }
}
