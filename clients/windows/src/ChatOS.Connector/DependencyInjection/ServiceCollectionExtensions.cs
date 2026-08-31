using ChatOS.Api.Http;
using ChatOS.Connector.Connection;
using ChatOS.Connector.Persistence;
using ChatOS.Connector.Security;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Workspaces;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.Terminal;
using ChatOS.Connector.Approval;
using ChatOS.Connector.Sandbox;
using ChatOS.Connector.Git;
using ChatOS.Connector.Remote;
using ChatOS.Connector.Plugins;
using ChatOS.Connector.NetworkGuard;
using ChatOS.Core.Abstractions;
using ChatOS.Core.State;
using Microsoft.Extensions.DependencyInjection;

namespace ChatOS.Connector.DependencyInjection;

public static class ServiceCollectionExtensions
{
    public static IServiceCollection AddChatOSConnector(this IServiceCollection services)
    {
        services.AddSingleton<IAuthTokenStore, WindowsCredentialTokenStore>();
        services.AddSingleton<IConnectorAccessTokenStore, WindowsCredentialConnectorTokenStore>();
        services.AddSingleton<IConnectorSecretStore, WindowsCredentialConnectorSecretStore>();
        services.AddSingleton<ConnectorDeviceIdentityProvider>();
        services.AddSingleton<ConnectorSocketRequestFactory>();
        services.AddSingleton<IConnectorSocketFactory, ClientWebSocketConnectorSocketFactory>();
        services.AddSingleton<LocalStateDatabase>();
        services.AddSingleton<IAppPreferencesStore, SqliteAppPreferencesStore>();
        services.AddSingleton<AppPreferencesManager>();
        services.AddSingleton<IPetActivitySuppressionStore, SqlitePetActivitySuppressionStore>();
        services.AddSingleton<IPetWindowPlacementStore, SqlitePetWindowPlacementStore>();
        services.AddSingleton<IPetFavoriteProjectsStore, SqlitePetFavoriteProjectsStore>();
        services.AddSingleton<PetFavoriteProjectsManager>();
        services.AddSingleton<IConversationCacheStore, SqliteConversationCacheStore>();
        services.AddSingleton<PetActivityCoordinator>();
        services.AddSingleton<ConnectorReconnectPolicy>();
        services.AddSingleton<ConnectorConnectionStateMachine>();
        services.AddSingleton<ConnectorPowerStateCoordinator>();
        services.AddSingleton<IConnectorPersistentStateStore, SqliteConnectorPersistentStateStore>();
        services.AddSingleton<IInstalledPluginStore, SqliteInstalledPluginStore>();
        services.AddSingleton<IPluginCredentialMetadataStore, SqlitePluginCredentialMetadataStore>();
        services.AddSingleton<PluginCredentialVault>();
        services.AddSingleton<IPluginOAuthConnectionStore, SqlitePluginOAuthConnectionStore>();
        services.AddSingleton<IExternalUriLauncher, WindowsExternalUriLauncher>();
        services.AddSingleton<PluginOAuthBroker>();
        services.AddSingleton<WindowsPluginPackageInstaller>();
        services.AddSingleton<ILocalPluginManagementService, LocalPluginManagementService>();
        services.AddSingleton<IPluginConfigurationService, PluginConfigurationService>();
        services.AddSingleton<PluginManifestLoader>();
        services.AddSingleton<IPluginMcpClientFactory, PluginMcpClientFactory>();
        services.AddSingleton<PluginRuntimeSessionStore>();
        services.AddSingleton<PluginArtifactRegistry>();
        services.AddSingleton<IPluginArtifactService>(provider =>
            provider.GetRequiredService<PluginArtifactRegistry>());
        services.AddSingleton<PluginVisualSessionReader>();
        services.AddSingleton<IPluginVisualSessionService>(provider =>
            provider.GetRequiredService<PluginVisualSessionReader>());
        services.AddSingleton<IPluginRuntimeLifetime>(provider =>
            provider.GetRequiredService<PluginRuntimeSessionStore>());
        services.AddSingleton<ConnectorRuntimeContext>();
        services.AddSingleton<IConnectorWorkspaceCatalog>(provider =>
            provider.GetRequiredService<ConnectorRuntimeContext>());
        services.AddSingleton<IConnectorWorkspaceContext>(provider =>
            provider.GetRequiredService<ConnectorRuntimeContext>());
        services.AddSingleton<ILocalProjectPathResolver, LocalProjectPathResolver>();
        services.AddSingleton<IProjectGitService, WindowsProjectGitService>();
        services.AddSingleton<IProjectCodeNavigationService, WindowsProjectCodeNavigationService>();
        services.AddSingleton<RemoteConnectionCredentialStore>();
        services.AddSingleton<IRemoteSshSessionFactory, SshNetRemoteSessionFactory>();
        services.AddSingleton<IRemoteConnectionTester, SshNetRemoteConnectionTester>();
        services.AddSingleton<WindowsRemoteConnectionService>();
        services.AddSingleton<IRemoteConnectionService>(provider => provider.GetRequiredService<WindowsRemoteConnectionService>());
        services.AddSingleton<IRemoteConnectionRuntime>(provider => provider.GetRequiredService<WindowsRemoteConnectionService>());
        services.AddSingleton<IRemoteSftpService, SshNetRemoteSftpService>();
        services.AddSingleton<IRemoteTerminalCommandService, SshNetRemoteTerminalCommandService>();
        services.AddSingleton<IRelaySecurityContextProvider>(provider =>
            provider.GetRequiredService<ConnectorRuntimeContext>());
        services.AddSingleton<IRelayRequestVerifier, Ed25519RelayRequestVerifier>();
        services.AddSingleton<IRelayRequestHandler, WorkspaceRelayHandler>();
        services.AddSingleton<IRelayRequestHandler, PluginRelayHandler>();
        services.AddSingleton<IRelayRequestHandler, PluginHttpMcpProxy>();
        services.AddSingleton<RelayDispatcher>();
        services.AddSingleton<ConnectorGatewayConnection>();
        services.AddHttpClient(ConnectorGatewayHttpClient.HttpClientName, client =>
        {
            client.Timeout = TimeSpan.FromSeconds(120);
        });
        services.AddHttpClient(OpenAiCompatibleCommandApprovalReviewer.HttpClientName, client =>
        {
            client.Timeout = TimeSpan.FromSeconds(45);
        }).ConfigurePrimaryHttpMessageHandler(() => new HttpClientHandler
        {
            AllowAutoRedirect = false,
            AutomaticDecompression = System.Net.DecompressionMethods.None,
            UseCookies = false,
        });
        services.AddHttpClient(PluginHttpMcpProxy.HttpClientName, client =>
        {
            client.Timeout = Timeout.InfiniteTimeSpan;
        }).ConfigurePrimaryHttpMessageHandler(() => new HttpClientHandler
        {
            AllowAutoRedirect = false,
            AutomaticDecompression = System.Net.DecompressionMethods.None,
            UseCookies = false,
        });
        services.AddHttpClient(PluginOAuthBroker.HttpClientName, client =>
        {
            client.Timeout = TimeSpan.FromSeconds(30);
        }).ConfigurePrimaryHttpMessageHandler(() => new HttpClientHandler
        {
            AllowAutoRedirect = false,
            AutomaticDecompression = System.Net.DecompressionMethods.None,
            UseCookies = false,
        });
        services.AddSingleton<IConnectorGatewayClient, ConnectorGatewayHttpClient>();
        services.AddSingleton<ConnectorPairingService>();
        services.AddSingleton<ILocalConnectorControlService, LocalConnectorControlService>();
        services.AddSingleton<ConnectorOutboundEventHub>();
        services.AddSingleton<IConnectorApprovalStore, SqliteConnectorApprovalStore>();
        services.AddSingleton<IConnectorModelSettingsStore, SqliteConnectorModelSettingsStore>();
        services.AddSingleton<IConnectorSandboxSettingsStore, SqliteConnectorSandboxSettingsStore>();
        services.AddSingleton<INetworkGuardTransport, NamedPipeNetworkGuardTransport>();
        services.AddSingleton<IControlledNetworkGuardClient>(provider =>
            new ControlledNetworkGuardClient(provider.GetRequiredService<INetworkGuardTransport>()));
        services.AddSingleton(provider => new NetworkGuardLeaseCoordinator(
            provider.GetRequiredService<IControlledNetworkGuardClient>()));
        services.AddSingleton<SandboxExecutionPolicyProvider>();
        services.AddSingleton<ApprovalModelRuntimeConfigurationService>();
        services.AddSingleton<IApprovalReviewerReadinessService, ApprovalReviewerReadinessService>();
        services.AddSingleton<ICommandApprovalAiReviewer, OpenAiCompatibleCommandApprovalReviewer>();
        services.AddSingleton<CommandApprovalCoordinator>();
        services.AddSingleton<CommandRiskEvaluator>();
        services.AddSingleton<ITerminalCommandExecutor, WindowsTerminalCommandExecutor>();
        services.AddSingleton<ITerminalCommandHistoryStore, SqliteTerminalCommandHistoryStore>();
        services.AddSingleton<ITerminalSessionFactory, ConPtyTerminalSessionFactory>();
        services.AddSingleton<TerminalSessionManager>();
        services.AddSingleton<TerminalRelayHandler>();
        services.AddSingleton<IRelayRequestHandler>(provider =>
            provider.GetRequiredService<TerminalRelayHandler>());
        services.AddSingleton<IRelayOneWayHandler>(provider =>
            provider.GetRequiredService<TerminalRelayHandler>());
        services.AddSingleton<ConnectorManagedConfigSynchronizer>();
        services.AddSingleton<IConnectorControlledNetworkReadinessService,
            ConnectorControlledNetworkReadinessService>();
        services.AddHostedService<ConnectorManagedConfigBackgroundService>();
        services.AddHostedService<ConnectorBackgroundService>();
        return services;
    }
}
