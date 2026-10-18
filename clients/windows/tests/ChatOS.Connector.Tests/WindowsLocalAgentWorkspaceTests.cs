using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentWorkspaceTests
{
    [Fact]
    public async Task ListsBoundConversationsAndCreatesSystemContactsLocally()
    {
        var host = new WorkspaceHost();
        host.Pages.Enqueue(new WindowsLocalConversationPage(
            [
                Record("unbound"),
                Record("project-conversation", "project", "project-1", 2_000),
            ],
            2_000,
            "project-conversation"));
        host.Pages.Enqueue(new WindowsLocalConversationPage([], null, null));
        var workspace = Service(host);
        workspace.Configure("user-1");

        var snapshot = await workspace.FetchWorkspaceRelationsAsync();

        Assert.Equal(
            ["jiguli", "plugin-installer"],
            snapshot.Contacts.Select(value => value.Id).ToArray());
        Assert.Collection(
            snapshot.Conversations.OrderBy(
                value => value.ProjectId is null ? value.ContactId : $"zz-{value.ProjectId}",
                StringComparer.Ordinal),
            contact =>
            {
                Assert.Equal("jiguli", contact.ContactId);
                Assert.Null(contact.ProjectId);
            },
            installer =>
            {
                Assert.Equal("plugin-installer", installer.ContactId);
                Assert.Equal("plugin-installer", installer.ContactAgentId);
                Assert.Null(installer.ProjectId);
            },
            project =>
            {
                Assert.Equal("project-1", project.ProjectId);
                Assert.Equal("jiguli", project.ContactAgentId);
            });
        Assert.Collection(
            host.Lists,
            first =>
            {
                Assert.Equal("user-1", first.OwnerUserId);
                Assert.Null(first.BeforeUpdatedAtUnixMs);
            },
            second =>
            {
                Assert.Equal(2_000, second.BeforeUpdatedAtUnixMs);
                Assert.Equal("project-conversation", second.BeforeConversationId);
            });
        Assert.Equal(
            ["jiguli", "plugin-installer"],
            host.Creates.Select(value => value.Resource!.ResourceId).ToArray());
        Assert.All(host.Creates, create => Assert.Equal("contact", create.Resource?.Kind));
    }

    [Fact]
    public async Task ReusesAnExistingProjectBinding()
    {
        var host = new WorkspaceHost();
        host.Pages.Enqueue(new WindowsLocalConversationPage(
            [Record("project-conversation", "project", "project-1")],
            null,
            null));
        var workspace = Service(host);
        var projectConversations = new WindowsLocalAgentProjectConversationService(
            new WindowsLocalAgentConversationClient(host),
            workspace);
        projectConversations.Configure("user-1");

        var conversationId = await projectConversations.EnsureConversationAsync(
            Project(),
            WindowsLocalAgentWorkspaceService.MainContact);

        Assert.Equal("project-conversation", conversationId);
        Assert.Empty(host.Creates);
    }

    [Fact]
    public async Task ResolvesAConcurrentProjectCreateFromTheUniqueBinding()
    {
        var host = new WorkspaceHost { ConflictOnCreate = true };
        host.Pages.Enqueue(new WindowsLocalConversationPage([], null, null));
        host.Pages.Enqueue(new WindowsLocalConversationPage(
            [Record("winning-conversation", "project", "project-1")],
            null,
            null));
        var workspace = Service(host);
        var projectConversations = new WindowsLocalAgentProjectConversationService(
            new WindowsLocalAgentConversationClient(host),
            workspace);
        projectConversations.Configure("user-1");

        var conversationId = await projectConversations.EnsureConversationAsync(
            Project(),
            WindowsLocalAgentWorkspaceService.MainContact);

        Assert.Equal("winning-conversation", conversationId);
        Assert.Single(host.Creates);
    }

    [Fact]
    public async Task RejectsAmbiguousProjectBindingsWithoutCreatingAnotherConversation()
    {
        var host = new WorkspaceHost();
        host.Pages.Enqueue(new WindowsLocalConversationPage(
            [
                Record("project-conversation-2", "project", "project-1", 2_000),
                Record("project-conversation-1", "project", "project-1", 1_000),
            ],
            null,
            null));
        var workspace = Service(host);
        var projectConversations = new WindowsLocalAgentProjectConversationService(
            new WindowsLocalAgentConversationClient(host),
            workspace);
        projectConversations.Configure("user-1");

        var error = await Assert.ThrowsAsync<InvalidOperationException>(() =>
            projectConversations.EnsureConversationAsync(
                Project(),
                WindowsLocalAgentWorkspaceService.MainContact));

        Assert.Contains("more than one", error.Message, StringComparison.Ordinal);
        Assert.Empty(host.Creates);
    }

    [Fact]
    public async Task RejectsConversationPagesFromAnotherOwner()
    {
        var host = new WorkspaceHost();
        host.Pages.Enqueue(new WindowsLocalConversationPage(
            [Record("foreign-project", "project", "project-1") with { OwnerUserId = "user-2" }],
            null,
            null));
        var workspace = Service(host);
        var projectConversations = new WindowsLocalAgentProjectConversationService(
            new WindowsLocalAgentConversationClient(host),
            workspace);
        projectConversations.Configure("user-1");

        var error = await Assert.ThrowsAsync<InvalidDataException>(() =>
            projectConversations.EnsureConversationAsync(
                Project(),
                WindowsLocalAgentWorkspaceService.MainContact));

        Assert.Contains("different owner", error.Message, StringComparison.Ordinal);
        Assert.Empty(host.Creates);
    }

    [Fact]
    public async Task RejectsRepeatedConversationPaginationCursor()
    {
        var host = new WorkspaceHost();
        host.Pages.Enqueue(new WindowsLocalConversationPage([], 2_000, "cursor"));
        host.Pages.Enqueue(new WindowsLocalConversationPage([], 2_000, "cursor"));
        var workspace = Service(host);
        workspace.Configure("user-1");

        var error = await Assert.ThrowsAsync<InvalidDataException>(() =>
            workspace.FetchWorkspaceRelationsAsync());

        Assert.Contains("repeated conversation cursor", error.Message, StringComparison.Ordinal);
        Assert.Equal(2, host.Lists.Count);
        Assert.Empty(host.Creates);
    }

    [Theory]
    [InlineData("owner")]
    [InlineData("conversation")]
    [InlineData("resource")]
    public async Task RejectsCreatedConversationWithMismatchedBinding(string mismatch)
    {
        var host = new WorkspaceHost { CreatedConversationMismatch = mismatch };
        host.Pages.Enqueue(new WindowsLocalConversationPage([], null, null));
        var workspace = Service(host);
        var projectConversations = new WindowsLocalAgentProjectConversationService(
            new WindowsLocalAgentConversationClient(host),
            workspace);
        projectConversations.Configure("user-1");

        var error = await Assert.ThrowsAsync<InvalidDataException>(() =>
            projectConversations.EnsureConversationAsync(
                Project(),
                WindowsLocalAgentWorkspaceService.MainContact));

        Assert.Contains("invalid conversation binding", error.Message, StringComparison.Ordinal);
        Assert.Single(host.Creates);
    }

    [Fact]
    public async Task RejectsCreatedSystemContactWithMismatchedOwner()
    {
        var host = new WorkspaceHost { CreatedConversationMismatch = "owner" };
        host.Pages.Enqueue(new WindowsLocalConversationPage([], null, null));
        var workspace = Service(host);
        workspace.Configure("user-1");

        var error = await Assert.ThrowsAsync<InvalidDataException>(() =>
            workspace.FetchWorkspaceRelationsAsync());

        Assert.Contains("invalid conversation binding", error.Message, StringComparison.Ordinal);
        Assert.Single(host.Creates);
    }

    private static WindowsLocalAgentWorkspaceService Service(WorkspaceHost host) =>
        new(new WindowsLocalAgentConversationClient(host));

    private static WorkspaceProject Project() => new(
        "project-1",
        "Project 1",
        null,
        null,
        null,
        new ProjectContextSnapshot(
            1,
            "project-1",
            "Project 1",
            1,
            new ProjectContextExecutionTarget("device-1", "workspace-1", "")));

    private static WindowsLocalConversationRecord Record(
        string id,
        string? resourceKind = null,
        string? resourceId = null,
        long updatedAtUnixMs = 1_000) => new(
        id,
        "user-1",
        id,
        1,
        1_000,
        updatedAtUnixMs,
        resourceKind is null || resourceId is null
            ? null
            : new WindowsLocalConversationResourceBinding(resourceKind, resourceId));

    private sealed class WorkspaceHost : ILocalAgentHostClient
    {
        public Queue<WindowsLocalConversationPage> Pages { get; } = new();
        public List<ListLocalConversationsCommand> Lists { get; } = [];
        public List<CreateLocalConversationCommand> Creates { get; } = [];
        public bool ConflictOnCreate { get; init; }
        public string? CreatedConversationMismatch { get; init; }
        public string? ActiveOwnerUserId => "user-1";

        public Task StartForOwnerAsync(
            string ownerUserId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> credentialEnvironment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default)
            where TCommand : notnull
        {
            object response = command switch
            {
                ListLocalConversationsCommand list => List(list),
                CreateLocalConversationCommand create => Create(create),
                _ => throw new InvalidOperationException(
                    $"Unexpected command: {typeof(TCommand).Name}"),
            };
            return Task.FromResult((TResponse)response);
        }

        private LocalConversationsResult List(ListLocalConversationsCommand command)
        {
            Lists.Add(command);
            return new("conversations", Pages.Dequeue());
        }

        private LocalConversationResult Create(CreateLocalConversationCommand command)
        {
            Creates.Add(command);
            if (ConflictOnCreate)
            {
                throw new LocalAgentHostRequestException("conflict", "duplicate", false);
            }
            return new("conversation", new WindowsLocalConversationDetail(
                Record(command.ConversationId) with
                {
                    ConversationId = CreatedConversationMismatch == "conversation"
                        ? "wrong-conversation"
                        : command.ConversationId,
                    OwnerUserId = CreatedConversationMismatch == "owner"
                        ? "user-2"
                        : command.OwnerUserId,
                    Title = command.Title,
                    Resource = CreatedConversationMismatch == "resource"
                        ? new WindowsLocalConversationResourceBinding("project", "wrong-project")
                        : command.Resource,
                },
                [],
                [],
                []));
        }
    }
}
