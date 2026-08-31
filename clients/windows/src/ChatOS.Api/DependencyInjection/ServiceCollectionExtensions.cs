using ChatOS.Api.Http;
using ChatOS.Api.Pet;
using ChatOS.Api.Authentication;
using ChatOS.Api.AskUser;
using ChatOS.Api.Conversation;
using ChatOS.Api.Realtime;
using ChatOS.Api.Projects;
using ChatOS.Api.Tasks;
using ChatOS.Api.Workspace;
using ChatOS.Api.Notepad;
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
            client.Timeout = TimeSpan.FromSeconds(60);
        });
        services.AddSingleton<IAuthenticationService, AuthenticationService>();
        services.AddSingleton<ILocalConnectorPairingTicketService, LocalConnectorPairingTicketService>();
        services.AddSingleton<IAskUserPromptService, AskUserPromptService>();
        services.AddHttpClient(ConversationAttachmentService.UploadClientName);
        services.AddSingleton<IConversationAttachmentService, ConversationAttachmentService>();
        services.AddSingleton<IConversationRuntimeSettingsService, ConversationRuntimeSettingsService>();
        services.AddSingleton<IConversationCommandService, ConversationCommandService>();
        services.AddSingleton<IConversationHistoryService, ConversationHistoryService>();
        services.AddSingleton<IWorkspaceService, WorkspaceService>();
        services.AddSingleton<IWorkspaceResourceCreationService, WorkspaceResourceCreationService>();
        services.AddSingleton<IRemoteConnectionCloudService, RemoteConnectionCloudService>();
        services.AddSingleton<IProjectFilesystemService, ProjectFilesystemService>();
        services.AddSingleton<IProjectPlanService, ProjectPlanService>();
        services.AddSingleton<IProjectExecutionService, ProjectExecutionService>();
        services.AddSingleton<IProjectRunService, ProjectRunService>();
        services.AddSingleton<INotepadService, NotepadService>();
        services.AddSingleton<IMessageTaskGraphService, MessageTaskGraphService>();
        services.AddSingleton<IPetActivityInboxService, PetActivityInboxService>();
        services.AddSingleton<WebSocketTicketService>();
        services.AddSingleton<IRealtimeClient, ChatOSRealtimeClient>();
        return services;
    }
}
