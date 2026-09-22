using ChatOS.Connector.Remote;

namespace ChatOS.Connector.Tests;

public sealed class SshVerificationExchangeTests
{
    [Fact]
    public async Task ChallengeResumesTheSameConnectionAttempt()
    {
        var attempt = new SshSessionAttempt<FakeSession>(
            initialCode: null,
            lifetime: TimeSpan.FromSeconds(5),
            async (verification, cancellationToken) =>
            {
                var response = await Task.Run(
                    () => verification.ResolveResponse("Please input MFA code (SMS):", null),
                    cancellationToken);
                return new FakeSession(response);
            });

        var challenge = await attempt.ObserveAsync(
            afterVersion: 0,
            phaseTimeout: TimeSpan.FromSeconds(2),
            CancellationToken.None);

        Assert.NotNull(challenge);
        Assert.Equal("Please input MFA code (SMS):", challenge.Prompt);
        Assert.False(attempt.Completion.IsCompleted);
        Assert.True(attempt.Submit(challenge.Version, " 614207 "));

        var nextChallenge = await attempt.ObserveAsync(
            challenge.Version,
            TimeSpan.FromSeconds(2),
            CancellationToken.None);
        Assert.Null(nextChallenge);
        using var session = await attempt.TakeAsync();
        Assert.Equal("614207", session.Response);
    }

    [Fact]
    public async Task InitialCodeIsConsumedOnceAndASecondPromptIsSurfaced()
    {
        using var lifetime = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        var exchange = new SshVerificationExchange(" 111111 ", lifetime.Token);

        Assert.Equal("111111", exchange.ResolveResponse("OTP:", null));
        var response = Task.Run(() => exchange.ResolveResponse("Second factor:", null));
        var challenge = await exchange.WaitForChallengeAfterAsync(0, lifetime.Token);

        Assert.Equal(1, challenge.Version);
        Assert.True(exchange.Submit(challenge.Version, "222222"));
        Assert.Equal("222222", await response.WaitAsync(TimeSpan.FromSeconds(2)));
    }

    [Fact]
    public void PasswordPromptDoesNotConsumeVerificationCode()
    {
        using var lifetime = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        var exchange = new SshVerificationExchange("654321", lifetime.Token);

        Assert.Equal("local-secret", exchange.ResolveResponse("Password:", "local-secret"));
        Assert.Equal("654321", exchange.ResolveResponse("Verification code:", null));
        Assert.Equal(0, exchange.Version);
    }

    private sealed class FakeSession(string response) : IDisposable
    {
        public string Response { get; } = response;
        public void Dispose() { }
    }
}
