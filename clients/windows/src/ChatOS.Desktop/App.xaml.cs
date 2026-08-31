using ChatOS.Api.DependencyInjection;
using ChatOS.Connector.DependencyInjection;
using ChatOS.Connector.Persistence;
using ChatOS.Desktop.AppShell;
using ChatOS.Desktop.Features.Chat;
using ChatOS.Desktop.Features.Projects;
using ChatOS.Desktop.Threading;
using ChatOS.Presentation.DependencyInjection;
using ChatOS.Presentation.Threading;
using ChatOS.Presentation.Settings;
using ChatOS.Core.State;
using ChatOS.Desktop.Features.Settings;
using ChatOS.Desktop.Features.Notepad;
using ChatOS.Desktop.Features.Remote;
using ChatOS.Desktop.Features.Pet;
using ChatOS.Desktop.Features.Plugins;
using ChatOS.Desktop.Features.Terminal;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.Windows.System.Power;
using ChatOS.Connector.Runtime;

namespace ChatOS.Desktop;

public partial class App : Application
{
    private readonly IHost _host;
    private Window? _window;
    private ConnectorPowerStateCoordinator? _powerState;

    public Window? MainWindow => _window;

    public App()
    {
        InitializeComponent();

        var builder = Host.CreateApplicationBuilder();
        var apiBaseUrl = Environment.GetEnvironmentVariable("CHATOS_API_BASE_URL");
        if (!string.IsNullOrWhiteSpace(apiBaseUrl))
        {
            builder.Configuration["ChatOS:Api:BaseUrl"] = apiBaseUrl;
        }

        builder.Services
            .AddChatOSApi(builder.Configuration)
            .AddChatOSConnector()
            .AddChatOSPresentation();
        builder.Services.AddSingleton<IUiDispatcher>(_ => new DispatcherQueueUiDispatcher(
            DispatcherQueue.GetForCurrentThread()
            ?? throw new InvalidOperationException("ChatOS must be launched on a UI thread.")));
        builder.Services.AddSingleton<MainWindowViewModel>();
        builder.Services.AddSingleton<ConversationPage>();
        builder.Services.AddSingleton<ProjectFilesPage>();
        builder.Services.AddSingleton<ProjectGitPage>();
        builder.Services.AddSingleton<ProjectPlanPage>();
        builder.Services.AddSingleton<ProjectRunPage>();
        builder.Services.AddSingleton<SettingsPage>();
        builder.Services.AddSingleton<PluginSettingsViewModel>();
        builder.Services.AddSingleton<ApprovalSettingsViewModel>();
        builder.Services.AddSingleton<ModelSettingsViewModel>();
        builder.Services.AddSingleton<SandboxSettingsViewModel>();
        builder.Services.AddSingleton<NotepadPage>();
        builder.Services.AddSingleton<RemoteConnectionsPage>();
        builder.Services.AddSingleton<RemoteSftpPage>();
        builder.Services.AddSingleton<RemoteTerminalPage>();
        builder.Services.AddSingleton<LocalTerminalPage>();
        builder.Services.AddSingleton<PetWindow>();
        builder.Services.AddSingleton<PetWindowController>();
        builder.Services.AddSingleton<PetQuickChatViewModel>();
        builder.Services.AddSingleton<PluginVisualSessionsViewModel>();
        builder.Services.AddSingleton<PluginArtifactsViewModel>();
        builder.Services.AddSingleton<IPluginArtifactUserInteraction, WindowsPluginArtifactUserInteraction>();
        builder.Services.AddSingleton<PluginArtifactsWindow>();
        builder.Services.AddSingleton<PluginVisualSessionWindow>();
        builder.Services.AddSingleton<PluginVisualSessionController>();
        builder.Services.AddSingleton<WorkspaceHostPage>();
        builder.Services.AddSingleton<MainWindow>();
        _host = builder.Build();
    }

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        await _host.Services.GetRequiredService<LocalStateDatabase>().InitializeAsync();
        await _host.Services.GetRequiredService<AppPreferencesManager>().InitializeAsync();
        await _host.Services.GetRequiredService<PetFavoriteProjectsManager>().InitializeAsync();
        Resources["ChatOSLocalization"] = _host.Services.GetRequiredService<LocalizationViewModel>();
        await _host.StartAsync();

        _powerState = _host.Services.GetRequiredService<ConnectorPowerStateCoordinator>();
        PowerManager.SystemSuspendStatusChanged += OnSystemSuspendStatusChanged;
        ApplySystemSuspendStatus();

        _window = _host.Services.GetRequiredService<MainWindow>();
        _window.Activate();
    }

    private void OnSystemSuspendStatusChanged(object? sender, object args) => ApplySystemSuspendStatus();

    private void ApplySystemSuspendStatus()
    {
        if (_powerState is null) return;
        if (PowerManager.SystemSuspendStatus == SystemSuspendStatus.Entering)
        {
            _powerState.Suspend();
        }
        else if (PowerManager.SystemSuspendStatus is SystemSuspendStatus.AutoResume or SystemSuspendStatus.ManualResume)
        {
            _powerState.Resume();
        }
    }
}
