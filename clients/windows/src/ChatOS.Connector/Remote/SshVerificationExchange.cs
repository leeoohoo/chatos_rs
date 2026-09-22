namespace ChatOS.Connector.Remote;

internal sealed record SshVerificationChallenge(long Version, string Prompt);

internal sealed class SshVerificationExchange
{
    private const int MaxChallenges = 4;
    private readonly object _sync = new();
    private readonly CancellationToken _lifetimeToken;
    private string? _initialCode;
    private long _version;
    private PendingChallenge? _active;
    private TaskCompletionSource _changed = NewSignal();

    public SshVerificationExchange(string? initialCode, CancellationToken lifetimeToken)
    {
        _initialCode = Clean(initialCode);
        _lifetimeToken = lifetimeToken;
    }

    public long Version
    {
        get
        {
            lock (_sync) return _version;
        }
    }

    public string ResolveResponse(string request, string? password)
    {
        if (LooksLikePassword(request) && Clean(password) is { } passwordValue)
        {
            return passwordValue;
        }

        PendingChallenge pending;
        TaskCompletionSource changed;
        lock (_sync)
        {
            if (_initialCode is { } code)
            {
                _initialCode = null;
                return code;
            }

            _version++;
            if (_version > MaxChallenges) return string.Empty;
            pending = new PendingChallenge(
                new SshVerificationChallenge(_version, NormalizePrompt(request)),
                new TaskCompletionSource<string>(TaskCreationOptions.RunContinuationsAsynchronously));
            _active = pending;
            changed = _changed;
            _changed = NewSignal();
        }
        changed.TrySetResult();

        try
        {
            return pending.Response.Task.WaitAsync(_lifetimeToken).GetAwaiter().GetResult();
        }
        catch (OperationCanceledException)
        {
            return string.Empty;
        }
        finally
        {
            lock (_sync)
            {
                if (ReferenceEquals(_active, pending)) _active = null;
            }
        }
    }

    public bool Submit(long version, string verificationCode)
    {
        var code = Clean(verificationCode);
        if (code is null) return false;
        lock (_sync)
        {
            return _active is { } active &&
                   active.Challenge.Version == version &&
                   active.Response.TrySetResult(code);
        }
    }

    public async Task<SshVerificationChallenge> WaitForChallengeAfterAsync(
        long version,
        CancellationToken cancellationToken)
    {
        while (true)
        {
            Task changed;
            lock (_sync)
            {
                if (_active is { } active && active.Challenge.Version > version)
                {
                    return active.Challenge;
                }
                changed = _changed.Task;
            }
            await changed.WaitAsync(cancellationToken).ConfigureAwait(false);
        }
    }

    internal static bool LooksLikePassword(string value) =>
        value.Contains("password", StringComparison.OrdinalIgnoreCase) ||
        value.Contains("密码", StringComparison.OrdinalIgnoreCase);

    private static string NormalizePrompt(string value) =>
        string.IsNullOrWhiteSpace(value) ? "请输入 SSH 二次验证码。" : value.Trim();

    private static string? Clean(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();

    private static TaskCompletionSource NewSignal() =>
        new(TaskCreationOptions.RunContinuationsAsynchronously);

    private sealed record PendingChallenge(
        SshVerificationChallenge Challenge,
        TaskCompletionSource<string> Response);
}

internal sealed class SshSessionAttempt<T> where T : class, IDisposable
{
    private readonly CancellationTokenSource _lifetime = new();
    private readonly SshVerificationExchange _verification;
    private int _claimed;
    private int _cancelled;
    private int _reaperStarted;

    public SshSessionAttempt(
        string? initialCode,
        TimeSpan lifetime,
        Func<SshVerificationExchange, CancellationToken, Task<T>> connect)
    {
        CreatedAt = DateTimeOffset.UtcNow;
        _lifetime.CancelAfter(lifetime);
        _verification = new SshVerificationExchange(initialCode, _lifetime.Token);
        Completion = connect(_verification, _lifetime.Token);
    }

    public DateTimeOffset CreatedAt { get; }
    public Task<T> Completion { get; }
    public long VerificationVersion => _verification.Version;
    public bool TryStartReaper() => Interlocked.Exchange(ref _reaperStarted, 1) == 0;

    public bool Submit(long version, string code) => _verification.Submit(version, code);

    public async Task<SshVerificationChallenge?> ObserveAsync(
        long afterVersion,
        TimeSpan phaseTimeout,
        CancellationToken cancellationToken)
    {
        using var phase = CancellationTokenSource.CreateLinkedTokenSource(
            cancellationToken,
            _lifetime.Token);
        phase.CancelAfter(phaseTimeout);
        var challenge = _verification.WaitForChallengeAfterAsync(afterVersion, phase.Token);
        var completed = await Task.WhenAny(Completion, challenge).ConfigureAwait(false);
        if (completed == Completion)
        {
            await Completion.ConfigureAwait(false);
            return null;
        }
        try
        {
            return await challenge.ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (
            !cancellationToken.IsCancellationRequested && !_lifetime.IsCancellationRequested)
        {
            throw new TimeoutException("SSH 连接超时，请检查网络和端口。");
        }
    }

    public async Task<T> TakeAsync()
    {
        var session = await Completion.ConfigureAwait(false);
        if (Interlocked.Exchange(ref _claimed, 1) != 0)
        {
            session.Dispose();
            throw new InvalidOperationException("SSH 会话已被其他请求接管。");
        }
        _lifetime.Dispose();
        return session;
    }

    public void Cancel()
    {
        if (Interlocked.Exchange(ref _cancelled, 1) != 0) return;
        try { _lifetime.Cancel(); } catch (ObjectDisposedException) { }
        _ = Completion.ContinueWith(
            task =>
            {
                if (task.Status == TaskStatus.RanToCompletion &&
                    Volatile.Read(ref _claimed) == 0)
                {
                    task.Result.Dispose();
                }
                _lifetime.Dispose();
            },
            CancellationToken.None,
            TaskContinuationOptions.ExecuteSynchronously,
            TaskScheduler.Default);
    }
}
