using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Desktop.AppShell;
using ChatOS.Desktop.Features.Pet;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Remote;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Connector.Tests;

public sealed class PetQuickChatViewModelTests
{
    [Fact]
    public async Task Jiguli_is_always_first_and_only_existing_favorite_projects_are_appended()
    {
        var dispatcher = new ImmediateUiDispatcher();
        var favorites = new PetFavoriteProjectsManager(new MemoryFavoriteStore(["project-two", "missing"]));
        await favorites.InitializeAsync();
        var main = CreateMainWindow(dispatcher);
        main.Contacts.Add(new ShellResourceViewModel(
            "contact-jiguli",
            WorkspaceResourceKind.Contact,
            "叽咕狸",
            "联系人",
            "glyph",
            "conversation-jiguli"));
        main.Projects.Add(new ShellResourceViewModel(
            "project-one",
            WorkspaceResourceKind.Project,
            "Project One",
            "C:\\one",
            "glyph",
            "conversation-one"));
        main.Projects.Add(new ShellResourceViewModel(
            "project-two",
            WorkspaceResourceKind.Project,
            "Project Two",
            "C:\\two",
            "glyph",
            "conversation-two"));
        var viewModel = new PetQuickChatViewModel(
            main,
            favorites,
            EmptyConversationFactory(dispatcher),
            Localization(dispatcher),
            dispatcher);

        Assert.Equal(["contact:contact-jiguli", "project:project-two"],
            viewModel.Resources.Select(value => value.Id));

        await favorites.SetFavoriteAsync("project-one", true);
        await favorites.SetFavoriteAsync("project-two", false);

        Assert.Equal(["contact:contact-jiguli", "project:project-one"],
            viewModel.Resources.Select(value => value.Id));
    }

    private static MainWindowViewModel CreateMainWindow(IUiDispatcher dispatcher) => new(
        null!,
        null!,
        null!,
        null!,
        null!,
        null!,
        null!,
        null!,
        null!,
        new RemoteConnectionsViewModel(null!, null!, dispatcher),
        Localization(dispatcher));

    private static ConversationSessionFactory EmptyConversationFactory(IUiDispatcher dispatcher) => new(
        null!,
        null!,
        null!,
        null!,
        null!,
        null!,
        new ConversationHistoryStore(),
        dispatcher);

    private static LocalizationViewModel Localization(IUiDispatcher dispatcher) => new(
        new AppPreferencesManager(new MemoryPreferencesStore()),
        dispatcher);

    private sealed class MemoryFavoriteStore(IReadOnlyList<string> values) : IPetFavoriteProjectsStore
    {
        private IReadOnlyList<string> _values = values;

        public Task<IReadOnlyList<string>> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(_values);

        public Task SaveAsync(IReadOnlyCollection<string> projectIds, CancellationToken cancellationToken = default)
        {
            _values = projectIds.ToArray();
            return Task.CompletedTask;
        }
    }

    private sealed class MemoryPreferencesStore : IAppPreferencesStore
    {
        public Task<AppPreferences?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<AppPreferences?>(AppPreferences.Default);

        public Task SaveAsync(AppPreferences preferences, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
    }
}
