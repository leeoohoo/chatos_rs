using System.Security.Cryptography;
using System.Text;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.Security;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.Runtime;

public sealed record ConnectorWorkspacePairing(
    string AbsoluteRoot,
    string? Alias = null);

public sealed record ConnectorPairingRequest(
    Uri GatewayBaseUri,
    string Ticket,
    string DeviceName,
    IReadOnlyList<ConnectorWorkspacePairing> Workspaces);

public sealed class ConnectorPairingService
{
    private readonly IConnectorGatewayClient _gateway;
    private readonly ConnectorDeviceIdentityProvider _identityProvider;
    private readonly IConnectorAccessTokenStore _tokens;
    private readonly ConnectorRuntimeContext _runtime;

    public ConnectorPairingService(
        IConnectorGatewayClient gateway,
        ConnectorDeviceIdentityProvider identityProvider,
        IConnectorAccessTokenStore tokens,
        ConnectorRuntimeContext runtime)
    {
        _gateway = gateway;
        _identityProvider = identityProvider;
        _tokens = tokens;
        _runtime = runtime;
    }

    public async Task<ConnectorPersistentState> PairAsync(
        ConnectorPairingRequest request,
        CancellationToken cancellationToken = default)
    {
        ValidateRequest(request);
        var deviceName = request.DeviceName.Trim();
        var login = await _gateway.ExchangeTicketAsync(
            request.GatewayBaseUri,
            request.Ticket.Trim(),
            deviceName,
            cancellationToken).ConfigureAwait(false);
        var identity = await _identityProvider.GetAsync(cancellationToken).ConfigureAwait(false);
        var device = await _gateway.CreateDeviceAsync(
            request.GatewayBaseUri,
            login.Token,
            deviceName,
            identity.PublicKey,
            cancellationToken).ConfigureAwait(false);
        ValidateDevice(device, login.User.Id, identity.PublicKey);

        var remoteWorkspaces = await _gateway.ListWorkspacesAsync(
            request.GatewayBaseUri,
            login.Token,
            cancellationToken).ConfigureAwait(false);
        var localWorkspaces = new List<ConnectorWorkspace>();
        foreach (var requestedWorkspace in NormalizeWorkspaces(request.Workspaces))
        {
            var fingerprint = Fingerprint(requestedWorkspace.Root, identity.PublicKey);
            var existing = remoteWorkspaces.FirstOrDefault(workspace =>
                string.Equals(
                    workspace.LocalPathFingerprint,
                    fingerprint,
                    StringComparison.Ordinal));
            ConnectorGatewayWorkspace remote;
            if (existing is null)
            {
                remote = await _gateway.CreateWorkspaceAsync(
                    request.GatewayBaseUri,
                    login.Token,
                    device.Id,
                    requestedWorkspace.Alias,
                    fingerprint,
                    cancellationToken).ConfigureAwait(false);
            }
            else if (!string.Equals(existing.DeviceId, device.Id, StringComparison.Ordinal))
            {
                remote = await _gateway.MoveWorkspaceAsync(
                    request.GatewayBaseUri,
                    login.Token,
                    existing.Id,
                    device.Id,
                    cancellationToken).ConfigureAwait(false);
            }
            else
            {
                remote = existing;
            }

            if (!string.Equals(remote.DeviceId, device.Id, StringComparison.Ordinal) ||
                !string.Equals(remote.LocalPathFingerprint, fingerprint, StringComparison.Ordinal))
            {
                throw new InvalidDataException("Connector gateway returned a mismatched workspace registration.");
            }

            localWorkspaces.Add(new ConnectorWorkspace(
                remote.Id,
                remote.LocalPathAlias,
                requestedWorkspace.Root,
                remote.LocalPathFingerprint));
        }

        var trust = await _gateway.GetRemoteControlTrustAsync(
            request.GatewayBaseUri,
            login.Token,
            cancellationToken).ConfigureAwait(false);
        var nextState = new ConnectorPersistentState(
            request.GatewayBaseUri,
            new ConnectorUser(
                login.User.Id,
                login.User.Username,
                login.User.DisplayName,
                login.User.Role),
            device.Id,
            device.DisplayName,
            localWorkspaces,
            trust);

        var previousToken = await _tokens.GetAccessTokenAsync(cancellationToken).ConfigureAwait(false);
        await _tokens.SetAccessTokenAsync(login.Token, cancellationToken).ConfigureAwait(false);
        try
        {
            var previousState = _runtime.Snapshot.State;
            await _runtime.ReplaceAsync(nextState, cancellationToken).ConfigureAwait(false);
            if (previousState is not null &&
                previousState.GatewayBaseUri == request.GatewayBaseUri &&
                !string.Equals(previousState.DeviceId, device.Id, StringComparison.Ordinal))
            {
                try
                {
                    await _gateway.DisconnectDeviceAsync(
                        previousState.GatewayBaseUri,
                        login.Token,
                        previousState.DeviceId,
                        cancellationToken).ConfigureAwait(false);
                }
                catch
                {
                    // New pairing is committed; stale-device cleanup can be retried later.
                }
            }
        }
        catch
        {
            if (string.IsNullOrWhiteSpace(previousToken))
            {
                await _tokens.ClearAsync(CancellationToken.None).ConfigureAwait(false);
            }
            else
            {
                await _tokens.SetAccessTokenAsync(previousToken, CancellationToken.None).ConfigureAwait(false);
            }

            throw;
        }

        return nextState;
    }

    private static void ValidateRequest(ConnectorPairingRequest request)
    {
        if (!request.GatewayBaseUri.IsAbsoluteUri ||
            request.GatewayBaseUri.Scheme is not ("http" or "https"))
        {
            throw new ArgumentException("Connector gateway must be an absolute HTTP(S) URL.", nameof(request));
        }

        if (string.IsNullOrWhiteSpace(request.Ticket) || string.IsNullOrWhiteSpace(request.DeviceName))
        {
            throw new ArgumentException("Pairing ticket and device name are required.", nameof(request));
        }

        if (request.Workspaces.Count == 0)
        {
            throw new ArgumentException("At least one local workspace is required.", nameof(request));
        }
    }

    private static void ValidateDevice(
        ConnectorGatewayDevice device,
        string ownerUserId,
        string publicKey)
    {
        if (device.OwnerUserId is not null &&
            !string.Equals(device.OwnerUserId, ownerUserId, StringComparison.Ordinal))
        {
            throw new InvalidDataException("Registered connector device belongs to another user.");
        }

        if (!string.Equals(device.PublicKey, publicKey, StringComparison.Ordinal))
        {
            throw new InvalidDataException("Registered connector device public key does not match this PC.");
        }
    }

    private static IReadOnlyList<(string Root, string Alias)> NormalizeWorkspaces(
        IReadOnlyList<ConnectorWorkspacePairing> workspaces)
    {
        var comparison = OperatingSystem.IsWindows()
            ? StringComparer.OrdinalIgnoreCase
            : StringComparer.Ordinal;
        var roots = new HashSet<string>(comparison);
        var normalized = new List<(string Root, string Alias)>();
        foreach (var workspace in workspaces)
        {
            var root = new WorkspacePathGuard(workspace.AbsoluteRoot).Root;
            if (!roots.Add(root))
            {
                continue;
            }

            var alias = !string.IsNullOrWhiteSpace(workspace.Alias)
                ? workspace.Alias.Trim()
                : DefaultAlias(root);
            normalized.Add((root, alias));
        }

        return normalized;
    }

    private static string DefaultAlias(string root)
    {
        var trimmed = Path.TrimEndingDirectorySeparator(root);
        var name = Path.GetFileName(trimmed);
        return string.IsNullOrWhiteSpace(name) ? root : name;
    }

    internal static string Fingerprint(string absoluteRoot, string publicKey)
    {
        var bytes = SHA256.HashData(Encoding.UTF8.GetBytes($"{absoluteRoot}\0{publicKey}"));
        return Convert.ToHexString(bytes).ToLowerInvariant();
    }
}
