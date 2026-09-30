using ChatOS.Api.Http;
using ChatOS.Api.Authentication;
using ChatOS.Api.Media;
using ChatOS.Core.Abstractions;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Options;

namespace ChatOS.Api.DependencyInjection;

public static class ServiceCollectionExtensions
{
    public static IServiceCollection AddChatOSApi(
        this IServiceCollection services,
        IConfiguration configuration)
    {
        services.Configure<ChatOSApiOptions>(configuration.GetSection(ChatOSApiOptions.SectionName));
        services.AddHttpClient<ChatOSApiClient>((provider, client) =>
        {
            var options = provider.GetRequiredService<IOptions<ChatOSApiOptions>>().Value;
            var baseUrl = options.BaseUrl.EndsWith("/", StringComparison.Ordinal)
                ? options.BaseUrl
                : $"{options.BaseUrl}/";
            client.BaseAddress = new Uri(baseUrl, UriKind.Absolute);
            // Per-request deadlines are enforced by ChatOSApiClient so Harness imports can
            // opt into a longer timeout without being preempted by HttpClient's global limit.
            client.Timeout = Timeout.InfiniteTimeSpan;
        });
        services.AddHttpClient(MediaGenerationService.ProviderClientName, client =>
            client.Timeout = Timeout.InfiniteTimeSpan);
        services.AddHttpClient(StoryPlanningService.ProviderClientName, client =>
            client.Timeout = Timeout.InfiniteTimeSpan);
        services.AddSingleton<IAuthenticationService, AuthenticationService>();
        services.AddSingleton<ILocalConnectorPairingTicketService, LocalConnectorPairingTicketService>();
        services.AddSingleton<IMediaGenerationService, MediaGenerationService>();
        services.AddSingleton<IStoryPlanningService, StoryPlanningService>();
        return services;
    }
}
