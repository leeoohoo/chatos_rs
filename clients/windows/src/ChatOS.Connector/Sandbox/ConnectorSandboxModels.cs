using ChatOS.Connector.NetworkGuard;

namespace ChatOS.Connector.Sandbox;

public enum ConnectorSandboxPermissionProfile
{
    ReadOnly,
    WorkspaceWrite,
    FullAccess,
}

public enum ConnectorSandboxNetworkAccess
{
    Disabled,
    Controlled,
    Host,
}

public sealed record ConnectorSandboxSettings(
    bool Enabled,
    ConnectorSandboxPermissionProfile PermissionProfile,
    ConnectorSandboxNetworkAccess NetworkAccess)
{
    public static ConnectorSandboxSettings Default { get; } = new(
        Enabled: true,
        ConnectorSandboxPermissionProfile.WorkspaceWrite,
        ConnectorSandboxNetworkAccess.Disabled);

    public ConnectorSandboxSettings Normalize()
    {
        if (!Enabled || PermissionProfile is ConnectorSandboxPermissionProfile.FullAccess)
        {
            return this with
            {
                Enabled = false,
                PermissionProfile = ConnectorSandboxPermissionProfile.FullAccess,
                NetworkAccess = ConnectorSandboxNetworkAccess.Host,
            };
        }

        return this;
    }
}

public interface IConnectorSandboxSettingsStore
{
    Task<ConnectorSandboxSettings> LoadAsync(CancellationToken cancellationToken = default);

    Task SaveAsync(
        ConnectorSandboxSettings settings,
        CancellationToken cancellationToken = default);
}

internal sealed record SandboxExecutionPolicy(
    bool UseAppContainer,
    ConnectorSandboxPermissionProfile PermissionProfile,
    ConnectorSandboxNetworkAccess NetworkAccess)
{
    public bool AllowHostNetwork => NetworkAccess is not ConnectorSandboxNetworkAccess.Disabled;

    public bool GrantInternetCapabilities => NetworkAccess is ConnectorSandboxNetworkAccess.Host;

    public static SandboxExecutionPolicy FromSettings(ConnectorSandboxSettings settings)
    {
        settings = settings.Normalize();
        return new SandboxExecutionPolicy(
            settings.Enabled,
            settings.PermissionProfile,
            settings.NetworkAccess);
    }
}

public sealed class SandboxExecutionPolicyProvider(
    IConnectorSandboxSettingsStore store,
    IControlledNetworkGuardClient? networkGuard = null)
{
    internal async Task<SandboxExecutionPolicy> ResolveAsync(
        CancellationToken cancellationToken = default)
    {
        var policy = SandboxExecutionPolicy.FromSettings(
            await store.LoadAsync(cancellationToken).ConfigureAwait(false));
        if (policy.NetworkAccess is ConnectorSandboxNetworkAccess.Controlled)
        {
            var readiness = networkGuard is null
                ? new NetworkGuardReadiness(NetworkGuardReadinessState.ServiceUnavailable)
                : await networkGuard.CheckReadinessAsync(cancellationToken).ConfigureAwait(false);
            if (!readiness.IsReady)
            {
                throw new InvalidOperationException(
                    $"Controlled-domain networking is unavailable ({readiness.State}).");
            }
        }
        return policy;
    }
}
