using ChatOS.Core.State;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Projects;
using ChatOS.Presentation.Tasks;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Notepad;
using ChatOS.Presentation.Remote;
using ChatOS.Presentation.Pet;
using Microsoft.Extensions.DependencyInjection;

namespace ChatOS.Presentation.DependencyInjection;

public static class ServiceCollectionExtensions
{
    public static IServiceCollection AddChatOSPresentation(this IServiceCollection services)
    {
        services.AddSingleton<ConversationHistoryStore>();
        services.AddSingleton<ConversationSessionViewModel>();
        services.AddSingleton<ConversationSessionFactory>();
        services.AddSingleton<ProjectFilesViewModel>();
        services.AddSingleton<ProjectGitViewModel>();
        services.AddSingleton<ProjectPlanViewModel>();
        services.AddSingleton<ProjectRunViewModel>();
        services.AddSingleton<MessageTaskGraphViewModel>();
        services.AddSingleton<LocalizationViewModel>();
        services.AddSingleton<AppSettingsViewModel>();
        services.AddSingleton<ConnectorSettingsViewModel>();
        services.AddSingleton<NotepadViewModel>();
        services.AddSingleton<RemoteConnectionsViewModel>();
        services.AddSingleton<RemoteSftpViewModel>();
        services.AddSingleton<RemoteTerminalViewModel>();
        services.AddSingleton<PetOverlayViewModel>();
        return services;
    }
}
