using System.Security.Cryptography;
using System.Text;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public enum WindowsLocalAgentAccountSessionFailure
{
    InvalidAccount,
    InvalidAccessToken,
    InvalidDeviceId,
    InvalidPersistentKey,
    CredentialUnavailable,
    Inactive,
    AccountMismatch,
    HostUnavailable,
}

public sealed class WindowsLocalAgentAccountSessionException(
    WindowsLocalAgentAccountSessionFailure failure,
    string message) : Exception(message)
{
    public WindowsLocalAgentAccountSessionFailure Failure { get; } = failure;
}

public interface IWindowsLocalAgentAccountSession : IAsyncDisposable
{
    Task ActivateAsync(string accountId, CancellationToken cancellationToken = default);

    Task UpdateAccessTokenAsync(string accountId, CancellationToken cancellationToken = default);

    Task LogoutAsync();

    Task<ILocalAgentIPCClient> GetClientAsync(
        string accountId,
        CancellationToken cancellationToken = default);

    Task<WindowsLocalAgentHostState> GetStateAsync();

    Task<IReadOnlyList<LocalAgentAttachmentReference>> StageAttachmentsAsync(
        string accountId,
        IReadOnlyList<ConversationAttachmentDraft> attachments,
        CancellationToken cancellationToken = default);

    Task DiscardStagedAttachmentsAsync(
        string accountId,
        IReadOnlyList<LocalAgentAttachmentReference> references);
}

/// Owns the one authenticated Windows Local Agent Host session. Common Agent
/// execution remains in clients/shared; this class only coordinates Windows
/// credentials, the process supervisor, and the current Named Pipe endpoint.
public sealed class WindowsLocalAgentAccountSession : IWindowsLocalAgentAccountSession
{
    public const string DeviceIdReference = "device-id";
    public const string SqliteEncryptionKeyReference = "sqlite-encryption-key";

    private const int PersistentKeyBytes = 32;
    private const int DeviceEntropyBytes = 16;
    private const int MaximumSecretBytes = 64 * 1024;
    private readonly IAuthTokenStore _tokens;
    private readonly IWindowsLocalAgentCredentialStore _credentials;
    private readonly IWindowsLocalAgentHostSupervisor _supervisor;
    private readonly WindowsLocalAgentHostBootstrapBuilder _builder;
    private readonly IWindowsLocalAgentRuntimeConfiguration _runtimeConfiguration;
    private readonly ILocalAgentIPCClientFactory _clientFactory;
    private readonly Func<int, byte[]> _randomBytes;
    private readonly WindowsLocalAgentAttachmentStager _attachmentStager = new();
    private readonly SemaphoreSlim _gate = new(1, 1);
    private string? _activeAccountId;
    private WindowsLocalAgentHostBootstrapSettings? _activeSettings;

    public WindowsLocalAgentAccountSession(
        IAuthTokenStore tokens,
        WindowsLocalAgentCredentialStore credentials,
        WindowsLocalAgentHostSupervisor supervisor,
        WindowsLocalAgentHostBootstrapBuilder builder,
        IWindowsLocalAgentRuntimeConfiguration runtimeConfiguration,
        ILocalAgentIPCClientFactory clientFactory)
        : this(
            tokens,
            credentials,
            supervisor,
            builder,
            runtimeConfiguration,
            clientFactory,
            RandomNumberGenerator.GetBytes)
    {
    }

    internal WindowsLocalAgentAccountSession(
        IAuthTokenStore tokens,
        IWindowsLocalAgentCredentialStore credentials,
        IWindowsLocalAgentHostSupervisor supervisor,
        WindowsLocalAgentHostBootstrapBuilder builder,
        IWindowsLocalAgentRuntimeConfiguration runtimeConfiguration,
        ILocalAgentIPCClientFactory clientFactory,
        Func<int, byte[]> randomBytes)
    {
        _tokens = tokens;
        _credentials = credentials;
        _supervisor = supervisor;
        _builder = builder;
        _runtimeConfiguration = runtimeConfiguration;
        _clientFactory = clientFactory;
        _randomBytes = randomBytes;
    }

    public async Task ActivateAsync(
        string accountId,
        CancellationToken cancellationToken = default)
    {
        ValidateIdentity(accountId);
        var token = await ReadAccessTokenAsync(cancellationToken).ConfigureAwait(false);
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await StopCoreAsync().ConfigureAwait(false);
            await StartCoreAsync(accountId, token, cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task UpdateAccessTokenAsync(
        string accountId,
        CancellationToken cancellationToken = default)
    {
        ValidateIdentity(accountId);
        var token = await ReadAccessTokenAsync(cancellationToken).ConfigureAwait(false);
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (!string.Equals(_activeAccountId, accountId, StringComparison.Ordinal))
            {
                throw Error(
                    WindowsLocalAgentAccountSessionFailure.AccountMismatch,
                    "The Local Agent account does not match the authenticated account.");
            }
            var settings = _activeSettings ?? throw Error(
                WindowsLocalAgentAccountSessionFailure.Inactive,
                "The Local Agent account session is inactive.");
            await SaveAccessTokenAsync(accountId, token, cancellationToken).ConfigureAwait(false);
            try
            {
                await StartSupervisorAsync(accountId, settings, cancellationToken)
                    .ConfigureAwait(false);
                _ = await CreateClientAsync(accountId, cancellationToken).ConfigureAwait(false);
            }
            catch
            {
                await FailClosedAsync(accountId).ConfigureAwait(false);
                throw;
            }
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task LogoutAsync()
    {
        await _gate.WaitAsync().ConfigureAwait(false);
        try
        {
            await StopCoreAsync().ConfigureAwait(false);
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task<ILocalAgentIPCClient> GetClientAsync(
        string accountId,
        CancellationToken cancellationToken = default)
    {
        ValidateIdentity(accountId);
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            return await CreateClientAsync(accountId, cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            _gate.Release();
        }
    }

    public Task<WindowsLocalAgentHostState> GetStateAsync() => _supervisor.GetStateAsync();

    public async Task<IReadOnlyList<LocalAgentAttachmentReference>> StageAttachmentsAsync(
        string accountId,
        IReadOnlyList<ConversationAttachmentDraft> attachments,
        CancellationToken cancellationToken = default)
    {
        ValidateIdentity(accountId);
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var settings = RequireActiveSettings(accountId);
            return await _attachmentStager.StageAsync(
                attachments,
                settings.AttachmentGrantDirectory,
                cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task DiscardStagedAttachmentsAsync(
        string accountId,
        IReadOnlyList<LocalAgentAttachmentReference> references)
    {
        ValidateIdentity(accountId);
        await _gate.WaitAsync().ConfigureAwait(false);
        try
        {
            if (string.Equals(_activeAccountId, accountId, StringComparison.Ordinal)
                && _activeSettings is { } settings)
            {
                _attachmentStager.Discard(references, settings.AttachmentGrantDirectory);
            }
        }
        finally
        {
            _gate.Release();
        }
    }

    public async ValueTask DisposeAsync()
    {
        await LogoutAsync().ConfigureAwait(false);
        _gate.Dispose();
    }

    private async Task StartCoreAsync(
        string accountId,
        string token,
        CancellationToken cancellationToken)
    {
        try
        {
            var deviceId = await PersistentDeviceIdAsync(accountId, cancellationToken)
                .ConfigureAwait(false);
            var settings = _runtimeConfiguration.Create(accountId, deviceId);
            if (!string.Equals(settings.AccountId, accountId, StringComparison.Ordinal))
            {
                throw Error(
                    WindowsLocalAgentAccountSessionFailure.InvalidAccount,
                    "The Local Agent runtime returned a different account identity.");
            }
            if (!string.Equals(settings.DeviceId, deviceId, StringComparison.Ordinal))
            {
                throw Error(
                    WindowsLocalAgentAccountSessionFailure.InvalidDeviceId,
                    "The Local Agent runtime returned a different device identity.");
            }

            await SaveAccessTokenAsync(accountId, token, cancellationToken).ConfigureAwait(false);
            await EnsurePersistentKeyAsync(
                accountId,
                WindowsLocalAgentHostBootstrapBuilder.ProviderContextKeyReference,
                cancellationToken).ConfigureAwait(false);
            if (settings.Storage is WindowsLocalAgentSqliteBootstrap sqlite)
            {
                await EnsurePersistentKeyAsync(
                    accountId,
                    sqlite.EncryptionSecretReference,
                    cancellationToken).ConfigureAwait(false);
            }

            await StartSupervisorAsync(accountId, settings, cancellationToken).ConfigureAwait(false);
            _activeAccountId = accountId;
            _activeSettings = settings;
            _ = await CreateClientAsync(accountId, cancellationToken).ConfigureAwait(false);
        }
        catch
        {
            await FailClosedAsync(accountId).ConfigureAwait(false);
            throw;
        }
    }

    private Task StartSupervisorAsync(
        string accountId,
        WindowsLocalAgentHostBootstrapSettings settings,
        CancellationToken cancellationToken) =>
        _supervisor.StartAsync(
            accountId,
            providerCancellation => BuildConfigurationAsync(
                accountId,
                settings,
                providerCancellation),
        cancellationToken);

    private WindowsLocalAgentHostBootstrapSettings RequireActiveSettings(string accountId)
    {
        if (!string.Equals(_activeAccountId, accountId, StringComparison.Ordinal))
        {
            throw Error(
                WindowsLocalAgentAccountSessionFailure.AccountMismatch,
                "The Local Agent account does not match the authenticated account.");
        }
        return _activeSettings ?? throw Error(
            WindowsLocalAgentAccountSessionFailure.Inactive,
            "The Local Agent account session is inactive.");
    }

    private async Task<WindowsLocalAgentHostLaunchConfiguration> BuildConfigurationAsync(
        string accountId,
        WindowsLocalAgentHostBootstrapSettings settings,
        CancellationToken cancellationToken)
    {
        var values = await LoadCredentialValuesAsync(accountId, settings.Storage, cancellationToken)
            .ConfigureAwait(false);
        try
        {
            var readOnlyValues = values.ToDictionary(
                pair => pair.Key,
                pair => (ReadOnlyMemory<byte>)pair.Value,
                StringComparer.Ordinal);
            return await _builder.BuildAsync(settings, readOnlyValues, cancellationToken)
                .ConfigureAwait(false);
        }
        finally
        {
            foreach (var value in values.Values)
            {
                CryptographicOperations.ZeroMemory(value);
            }
        }
    }

    private async Task<Dictionary<string, byte[]>> LoadCredentialValuesAsync(
        string accountId,
        WindowsLocalAgentStorageBootstrap storage,
        CancellationToken cancellationToken)
    {
        var values = new Dictionary<string, byte[]>(StringComparer.Ordinal);
        try
        {
            var token = await _credentials.LoadCredentialAsync(
                accountId,
                WindowsLocalAgentHostBootstrapBuilder.ModelAccessTokenReference,
                cancellationToken).ConfigureAwait(false);
            if (!ValidSecret(token))
            {
                throw Error(
                    WindowsLocalAgentAccountSessionFailure.InvalidAccessToken,
                    "The Local Agent access token is unavailable.");
            }
            values[WindowsLocalAgentHostBootstrapBuilder.ModelAccessTokenReference] =
                Encoding.UTF8.GetBytes(token!);

            var providerKey = await RequiredDeviceKeyAsync(
                accountId,
                WindowsLocalAgentHostBootstrapBuilder.ProviderContextKeyReference,
                cancellationToken).ConfigureAwait(false);
            values[WindowsLocalAgentHostBootstrapBuilder.ProviderContextKeyReference] = providerKey;

            switch (storage)
            {
                case WindowsLocalAgentSqliteBootstrap sqlite:
                    values[sqlite.EncryptionSecretReference] = await RequiredDeviceKeyAsync(
                        accountId,
                        sqlite.EncryptionSecretReference,
                        cancellationToken).ConfigureAwait(false);
                    break;
                case WindowsLocalAgentPostgresBootstrap postgres:
                    var secret = await _credentials.LoadCredentialAsync(
                        accountId,
                        postgres.ConnectionSecretReference,
                        cancellationToken).ConfigureAwait(false);
                    if (!ValidSecret(secret))
                    {
                        throw Error(
                            WindowsLocalAgentAccountSessionFailure.CredentialUnavailable,
                            $"The Local Agent credential '{postgres.ConnectionSecretReference}' is unavailable.");
                    }
                    values[postgres.ConnectionSecretReference] = Encoding.UTF8.GetBytes(secret!);
                    break;
                default:
                    throw new InvalidOperationException("The Local Agent storage profile is invalid.");
            }
            return values;
        }
        catch
        {
            foreach (var value in values.Values)
            {
                CryptographicOperations.ZeroMemory(value);
            }
            throw;
        }
    }

    private async Task<byte[]> RequiredDeviceKeyAsync(
        string accountId,
        string reference,
        CancellationToken cancellationToken)
    {
        var value = await _credentials.LoadDeviceKeyAsync(accountId, reference, cancellationToken)
            .ConfigureAwait(false);
        if (value?.Length != PersistentKeyBytes)
        {
            if (value is not null)
            {
                CryptographicOperations.ZeroMemory(value);
            }
            throw Error(
                WindowsLocalAgentAccountSessionFailure.InvalidPersistentKey,
                $"The Local Agent persistent key '{reference}' is invalid.");
        }
        return value;
    }

    private async Task EnsurePersistentKeyAsync(
        string accountId,
        string reference,
        CancellationToken cancellationToken)
    {
        var existing = await _credentials.LoadDeviceKeyAsync(accountId, reference, cancellationToken)
            .ConfigureAwait(false);
        if (existing is not null)
        {
            try
            {
                if (existing.Length != PersistentKeyBytes)
                {
                    throw Error(
                        WindowsLocalAgentAccountSessionFailure.InvalidPersistentKey,
                        $"The Local Agent persistent key '{reference}' is invalid.");
                }
                return;
            }
            finally
            {
                CryptographicOperations.ZeroMemory(existing);
            }
        }

        var generated = _randomBytes(PersistentKeyBytes);
        try
        {
            if (generated.Length != PersistentKeyBytes)
            {
                throw Error(
                    WindowsLocalAgentAccountSessionFailure.InvalidPersistentKey,
                    $"The Local Agent persistent key '{reference}' could not be generated.");
            }
            await _credentials.SaveDeviceKeyAsync(accountId, reference, generated, cancellationToken)
                .ConfigureAwait(false);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(generated);
        }
    }

    private async Task<string> PersistentDeviceIdAsync(
        string accountId,
        CancellationToken cancellationToken)
    {
        var existing = await _credentials.LoadCredentialAsync(
            accountId,
            DeviceIdReference,
            cancellationToken).ConfigureAwait(false);
        if (existing is not null)
        {
            if (!ValidDeviceId(existing))
            {
                throw Error(
                    WindowsLocalAgentAccountSessionFailure.InvalidDeviceId,
                    "The Local Agent device identity is invalid.");
            }
            return existing;
        }

        var entropy = _randomBytes(DeviceEntropyBytes);
        try
        {
            if (entropy.Length != DeviceEntropyBytes)
            {
                throw Error(
                    WindowsLocalAgentAccountSessionFailure.InvalidDeviceId,
                    "The Local Agent device identity could not be generated.");
            }
            var deviceId = "device-" + Convert.ToHexString(entropy).ToLowerInvariant();
            await _credentials.SaveCredentialAsync(
                accountId,
                DeviceIdReference,
                deviceId,
                cancellationToken).ConfigureAwait(false);
            return deviceId;
        }
        finally
        {
            CryptographicOperations.ZeroMemory(entropy);
        }
    }

    private async Task<ILocalAgentIPCClient> CreateClientAsync(
        string accountId,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (_activeAccountId is null)
        {
            throw Error(
                WindowsLocalAgentAccountSessionFailure.Inactive,
                "The Local Agent account session is inactive.");
        }
        if (!string.Equals(_activeAccountId, accountId, StringComparison.Ordinal))
        {
            throw Error(
                WindowsLocalAgentAccountSessionFailure.AccountMismatch,
                "The Local Agent account does not match the authenticated account.");
        }
        var state = await _supervisor.GetStateAsync().ConfigureAwait(false);
        if (state.Status != WindowsLocalAgentHostStatus.Running
            || !string.Equals(state.AccountId, accountId, StringComparison.Ordinal)
            || string.IsNullOrWhiteSpace(state.ClientEndpoint))
        {
            throw Error(
                WindowsLocalAgentAccountSessionFailure.HostUnavailable,
                "The Local Agent Host is not ready.");
        }
        return _clientFactory.Create(accountId, state.ClientEndpoint);
    }

    private async Task StopCoreAsync()
    {
        var accountId = _activeAccountId;
        _activeAccountId = null;
        _activeSettings = null;
        await _supervisor.LogoutAsync().ConfigureAwait(false);
        if (accountId is not null)
        {
            await _credentials.DeleteCredentialAsync(
                accountId,
                WindowsLocalAgentHostBootstrapBuilder.ModelAccessTokenReference)
                .ConfigureAwait(false);
        }
    }

    private async Task FailClosedAsync(string accountId)
    {
        _activeAccountId = null;
        _activeSettings = null;
        await _supervisor.LogoutAsync().ConfigureAwait(false);
        await _credentials.DeleteCredentialAsync(
            accountId,
            WindowsLocalAgentHostBootstrapBuilder.ModelAccessTokenReference)
            .ConfigureAwait(false);
    }

    private async Task<string> ReadAccessTokenAsync(CancellationToken cancellationToken)
    {
        var token = (await _tokens.GetAccessTokenAsync(cancellationToken).ConfigureAwait(false))?.Trim();
        if (!ValidSecret(token))
        {
            throw Error(
                WindowsLocalAgentAccountSessionFailure.InvalidAccessToken,
                "The authenticated access token is unavailable.");
        }
        return token!;
    }

    private Task SaveAccessTokenAsync(
        string accountId,
        string token,
        CancellationToken cancellationToken) =>
        _credentials.SaveCredentialAsync(
            accountId,
            WindowsLocalAgentHostBootstrapBuilder.ModelAccessTokenReference,
            token,
            cancellationToken).AsTask();

    private static void ValidateIdentity(string accountId)
    {
        if (string.IsNullOrWhiteSpace(accountId)
            || accountId != accountId.Trim()
            || accountId.Length > 512
            || accountId.Any(char.IsControl))
        {
            throw Error(
                WindowsLocalAgentAccountSessionFailure.InvalidAccount,
                "The Local Agent account identity is invalid.");
        }
    }

    private static bool ValidSecret(string? value) =>
        !string.IsNullOrWhiteSpace(value)
        && value == value.Trim()
        && Encoding.UTF8.GetByteCount(value) <= MaximumSecretBytes
        && !value.Any(char.IsControl);

    private static bool ValidDeviceId(string value) =>
        value.Length == 39
        && value.StartsWith("device-", StringComparison.Ordinal)
        && value["device-".Length..].All(character =>
            char.IsAsciiDigit(character) || character is >= 'a' and <= 'f');

    private static WindowsLocalAgentAccountSessionException Error(
        WindowsLocalAgentAccountSessionFailure failure,
        string message) => new(failure, message);
}
