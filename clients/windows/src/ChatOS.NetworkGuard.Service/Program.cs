using ChatOS.NetworkGuard.Service;

var builder = Host.CreateApplicationBuilder(args);
builder.Services.AddWindowsService(options =>
{
    options.ServiceName = "ChatOS NetworkGuard";
});
builder.Services.AddOptions<NetworkGuardServiceOptions>()
    .Bind(builder.Configuration.GetSection("ChatOS:NetworkGuard"))
    .Validate(options => options.TrustedPolicyPublicKeys.Count > 0,
        "At least one trusted controlled-network signing key is required.")
    .Validate(options => options.LeaseDuration >= TimeSpan.FromSeconds(30) &&
        options.LeaseDuration <= TimeSpan.FromMinutes(5),
        "LeaseDuration must be between 30 seconds and 5 minutes.")
    .Validate(options => options.HttpBrokerPort is >= 1024 and <= 65535 &&
        options.HttpsBrokerPort is >= 1024 and <= 65535 &&
        options.HttpBrokerPort != options.HttpsBrokerPort,
        "Broker ports must be distinct unprivileged TCP ports.")
    .Validate(options => options.HandshakeTimeout > TimeSpan.Zero &&
        options.ConnectTimeout > TimeSpan.Zero,
        "Broker timeouts must be positive.")
    .ValidateOnStart();
builder.Services.AddSingleton<NetworkGuardBrokerState>();
builder.Services.AddSingleton<INetworkGuardNativeController, WindowsNetworkGuardNativeController>();
builder.Services.AddSingleton<NetworkGuardDriverBackend>();
builder.Services.AddSingleton<INetworkGuardDriverBackend>(serviceProvider =>
    serviceProvider.GetRequiredService<NetworkGuardDriverBackend>());
builder.Services.AddSingleton<INetworkGuardLeasePolicyStore>(serviceProvider =>
    serviceProvider.GetRequiredService<NetworkGuardDriverBackend>());
builder.Services.AddSingleton<INetworkGuardRedirectContextResolver, WindowsWfpRedirectContextResolver>();
builder.Services.AddSingleton<INetworkGuardAddressResolver, SystemNetworkGuardAddressResolver>();
builder.Services.AddSingleton<INetworkGuardUpstreamConnector, NetworkGuardUpstreamConnector>();
builder.Services.AddSingleton<NetworkGuardBrokerConnectionHandler>();
builder.Services.AddSingleton<INetworkGuardProcessIdentityVerifier, WindowsNetworkGuardProcessIdentityVerifier>();
builder.Services.AddSingleton<NetworkGuardRequestHandler>();
builder.Services.AddHostedService<NetworkGuardStartupHostedService>();
builder.Services.AddHostedService<NetworkGuardBrokerHostedService>();
builder.Services.AddHostedService<NetworkGuardPipeHostedService>();
await builder.Build().RunAsync();
