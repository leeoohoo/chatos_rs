using ChatOS.NetworkGuard.Contracts;
using ChatOS.NetworkGuard.Service;
using Microsoft.Extensions.Options;
using Org.BouncyCastle.Crypto.Parameters;
using Org.BouncyCastle.Math.EC.Rfc8032;
using Org.BouncyCastle.Security;

namespace ChatOS.NetworkGuard.Tests;

public sealed class NetworkGuardRequestHandlerTests
{
    [Fact]
    public async Task HealthReportsDriverUnavailableWithoutPretendingReady()
    {
        var context = new TestContext(new StubDriver
        {
            Health = new NetworkGuardDriverHealth(false, false),
        });

        var response = await context.HandleAsync(context.Request(NetworkGuardOperation.Health));

        Assert.False(response.Success);
        Assert.Equal("driver_unavailable", response.FailureCode);
        Assert.False(response.DriverReady);
        Assert.False(response.SelfTestPassed);
    }

    [Fact]
    public async Task HealthyDriverReturnsSuccessWithoutFailureCode()
    {
        var context = new TestContext(new StubDriver
        {
            Health = new NetworkGuardDriverHealth(true, true, "test-driver", ActiveLeaseCount: 3),
        });

        var response = await context.HandleAsync(context.Request(NetworkGuardOperation.Health));

        Assert.True(response.Success);
        Assert.Null(response.FailureCode);
        Assert.Equal(3, response.ActiveLeaseCount);
    }

    [Theory]
    [InlineData(0, "correlation")]
    [InlineData(2, "correlation")]
    [InlineData(1, "")]
    [InlineData(1, "bad\ncorrelation")]
    public async Task ProtocolAndCorrelationAreValidated(int major, string correlation)
    {
        var context = new TestContext(new StubDriver());
        var request = context.Request(NetworkGuardOperation.Health) with
        {
            ProtocolMajor = major,
            CorrelationId = correlation,
        };

        var response = await context.HandleAsync(request);

        Assert.False(response.Success);
        Assert.Equal("protocol_mismatch", response.FailureCode);
        Assert.Equal(correlation, response.CorrelationId);
    }

    [Fact]
    public async Task ValidSignedPolicyIsPassedToDriver()
    {
        var driver = new StubDriver();
        var context = new TestContext(driver);
        var request = context.Request(NetworkGuardOperation.AcquireLease) with
        {
            Policy = context.SignPolicy(),
            AppContainerSid = TestContext.AppContainerSid,
            ProcessId = 421,
        };

        var response = await context.HandleAsync(request);

        Assert.True(response.Success);
        Assert.Equal("lease-1", response.LeaseId);
        Assert.Equal("workspace-1", driver.AcquiredPolicy?.WorkspaceId);
        Assert.Equal(TestContext.AppContainerSid, driver.AcquiredSid);
        Assert.Equal(421, driver.AcquiredProcessId);
    }

    [Fact]
    public async Task TamperedPolicyIsRejectedBeforeDriver()
    {
        var driver = new StubDriver();
        var context = new TestContext(driver);
        var signed = context.SignPolicy();
        var request = context.Request(NetworkGuardOperation.AcquireLease) with
        {
            Policy = signed with { AllowedHosts = ["evil.example.com"] },
            AppContainerSid = TestContext.AppContainerSid,
            ProcessId = 421,
        };

        var response = await context.HandleAsync(request);

        Assert.False(response.Success);
        Assert.Equal("policy_rejected", response.FailureCode);
        Assert.Null(driver.AcquiredPolicy);
    }

    [Fact]
    public async Task PolicyForAnotherWindowsUserIsRejectedBeforeDriver()
    {
        var driver = new StubDriver();
        var context = new TestContext(driver);
        var request = context.Request(NetworkGuardOperation.AcquireLease) with
        {
            Policy = context.SignPolicy(),
            AppContainerSid = TestContext.AppContainerSid,
            ProcessId = 421,
        };

        var response = await context.Handler.HandleAsync(
            request,
            new NetworkGuardCallerIdentity("S-1-5-21-999-888-777-666", 88));

        Assert.False(response.Success);
        Assert.Equal("policy_rejected", response.FailureCode);
        Assert.Null(driver.AcquiredPolicy);
    }

    [Fact]
    public async Task RenewAndReleaseKeepLeaseIdentity()
    {
        var driver = new StubDriver();
        var context = new TestContext(driver);
        var renew = context.Request(NetworkGuardOperation.RenewLease) with
        {
            LeaseId = "lease-bound",
            AppContainerSid = TestContext.AppContainerSid,
            ProcessId = 778,
        };

        var renewed = await context.HandleAsync(renew);
        var released = await context.HandleAsync(renew with
        {
            Operation = NetworkGuardOperation.ReleaseLease,
            CorrelationId = "release",
        });

        Assert.True(renewed.Success);
        Assert.True(released.Success);
        Assert.Equal(("lease-bound", TestContext.AppContainerSid, 778), driver.RenewedIdentity);
        Assert.Equal(("lease-bound", TestContext.AppContainerSid, 778), driver.ReleasedIdentity);
    }

    [Fact]
    public async Task BackendExceptionReturnsStableCodeWithoutLeakingMessage()
    {
        var context = new TestContext(new StubDriver
        {
            AcquireException = new Exception("authorization=Bearer secret-body"),
        });
        var request = context.Request(NetworkGuardOperation.AcquireLease) with
        {
            Policy = context.SignPolicy(),
            AppContainerSid = TestContext.AppContainerSid,
            ProcessId = 421,
        };

        var response = await context.HandleAsync(request);
        var serialized = System.Text.Json.JsonSerializer.Serialize(response);

        Assert.False(response.Success);
        Assert.Equal("operation_failed", response.FailureCode);
        Assert.DoesNotContain("secret-body", serialized, StringComparison.Ordinal);
        Assert.DoesNotContain("Bearer", serialized, StringComparison.Ordinal);
    }

    private sealed class TestContext
    {
        public const string AppContainerSid = "S-1-15-2-123456789-987654321";
        public const string WindowsUserSid = "S-1-5-21-100-200-300-400";
        private readonly Ed25519PrivateKeyParameters _privateKey = new(new SecureRandom());
        private readonly FrozenTimeProvider _time = new(DateTimeOffset.Parse("2026-08-30T12:00:00Z"));

        public TestContext(INetworkGuardDriverBackend driver)
        {
            var publicKey = Base64Url(_privateKey.GeneratePublicKey().GetEncoded());
            Handler = new NetworkGuardRequestHandler(
                driver,
                Options.Create(new NetworkGuardServiceOptions
                {
                    TrustedPolicyPublicKeys = new Dictionary<string, string>
                    {
                        ["key-1"] = "ed25519:" + publicKey,
                    },
                }),
                new AcceptingProcessIdentityVerifier(),
                _time);
        }

        public NetworkGuardRequestHandler Handler { get; }

        public Task<NetworkGuardResponse> HandleAsync(NetworkGuardRequest request) =>
            Handler.HandleAsync(request, new NetworkGuardCallerIdentity(WindowsUserSid, 88));

        public NetworkGuardRequest Request(NetworkGuardOperation operation) => new(
            NetworkGuardProtocol.MajorVersion,
            NetworkGuardProtocol.MinorVersion,
            "correlation",
            operation);

        public ControlledNetworkPolicyEnvelope SignPolicy()
        {
            var policy = new ControlledNetworkPolicy(
                "policy-1",
                "owner-1",
                "device-1",
                "workspace-1",
                WindowsUserSid,
                ["api.example.com"],
                [443],
                _time.GetUtcNow().AddHours(1),
                "key-1");
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
                policy.AllowedHosts,
                policy.AllowedPorts,
                policy.ExpiresAt,
                policy.SignatureKeyId,
                "ed25519",
                Base64Url(signature));
        }

        private static string Base64Url(byte[] value) =>
            Convert.ToBase64String(value).TrimEnd('=').Replace('+', '-').Replace('/', '_');
    }

    private sealed class FrozenTimeProvider(DateTimeOffset now) : TimeProvider
    {
        public override DateTimeOffset GetUtcNow() => now;
    }

    private sealed class AcceptingProcessIdentityVerifier : INetworkGuardProcessIdentityVerifier
    {
        public bool Verify(int processId, string expectedWindowsUserSid, string expectedAppContainerSid) =>
            processId > 0 && expectedWindowsUserSid == TestContext.WindowsUserSid &&
            expectedAppContainerSid == TestContext.AppContainerSid;
    }

    private sealed class StubDriver : INetworkGuardDriverBackend
    {
        public NetworkGuardDriverHealth Health { get; init; } = new(true, true, "driver-test");
        public Exception? AcquireException { get; init; }
        public ControlledNetworkPolicy? AcquiredPolicy { get; private set; }
        public string? AcquiredSid { get; private set; }
        public int? AcquiredProcessId { get; private set; }
        public (string LeaseId, string Sid, int ProcessId)? RenewedIdentity { get; private set; }
        public (string LeaseId, string Sid, int ProcessId)? ReleasedIdentity { get; private set; }

        public Task<NetworkGuardDriverHealth> CheckHealthAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(Health);

        public Task<NetworkGuardDriverLease> AcquireAsync(
            ControlledNetworkPolicy policy,
            string appContainerSid,
            int processId,
            string callerWindowsUserSid,
            CancellationToken cancellationToken = default)
        {
            if (AcquireException is not null)
            {
                return Task.FromException<NetworkGuardDriverLease>(AcquireException);
            }
            AcquiredPolicy = policy;
            AcquiredSid = appContainerSid;
            AcquiredProcessId = processId;
            return Task.FromResult(new NetworkGuardDriverLease("lease-1", policy.ExpiresAt));
        }

        public Task<NetworkGuardDriverLease> RenewAsync(
            string leaseId,
            string appContainerSid,
            int processId,
            string callerWindowsUserSid,
            CancellationToken cancellationToken = default)
        {
            RenewedIdentity = (leaseId, appContainerSid, processId);
            return Task.FromResult(new NetworkGuardDriverLease(
                leaseId,
                DateTimeOffset.UtcNow.AddMinutes(5)));
        }

        public Task ReleaseAsync(
            string leaseId,
            string appContainerSid,
            int processId,
            string callerWindowsUserSid,
            CancellationToken cancellationToken = default)
        {
            ReleasedIdentity = (leaseId, appContainerSid, processId);
            return Task.CompletedTask;
        }
    }
}
