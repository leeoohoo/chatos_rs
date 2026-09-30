using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentNotepadTests
{
    [Fact]
    public async Task NoteLifecycleUsesOwnerScopeAndOptimisticVersions()
    {
        var host = new NotepadHost();
        var service = new WindowsLocalAgentNotepadService(
            new WindowsLocalAgentNotepadClient(host));
        service.Configure("user-1");

        await service.InitializeAsync();
        await service.CreateFolderAsync("work/ideas");
        var created = await service.CreateNoteAsync(new NotepadNoteDraft(
            "work/ideas",
            "Local",
            "body",
            ["rust"]));
        Assert.Equal("note-1", created.Note.Id);

        var updated = await service.UpdateNoteAsync(
            "note-1",
            new NotepadNoteUpdate(Title: "Updated"));
        Assert.Equal("Updated", updated.Note.Title);
        var update = Assert.IsType<LocalNotepadUpdateNoteCommand>(host.LastCommand);
        Assert.Equal("user-1", update.OwnerUserId);
        Assert.Equal((ulong)1, update.ExpectedVersion);

        await service.DeleteNoteAsync("note-1");
        var delete = Assert.IsType<LocalNotepadDeleteNoteCommand>(host.LastCommand);
        Assert.Equal((ulong)2, delete.ExpectedVersion);
    }

    [Fact]
    public async Task FolderMutationInvalidatesCachedNoteVersion()
    {
        var host = new NotepadHost();
        var service = new WindowsLocalAgentNotepadService(
            new WindowsLocalAgentNotepadClient(host));
        service.Configure("user-1");
        _ = await service.FetchNoteAsync("note-1");

        await service.RenameFolderAsync("work", "archive");
        await service.UpdateNoteAsync("note-1", new NotepadNoteUpdate(Content: "new body"));

        Assert.Equal(1, host.GetCountAfterRename);
        var update = Assert.IsType<LocalNotepadUpdateNoteCommand>(host.LastCommand);
        Assert.Equal((ulong)2, update.ExpectedVersion);
    }

    private sealed class NotepadHost : ILocalAgentHostClient
    {
        private ulong _version = 1;
        private bool _renamed;

        public object? LastCommand { get; private set; }

        public int GetCountAfterRename { get; private set; }

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
            LastCommand = command;
            object response = command switch
            {
                LocalNotepadOwnerCommand => new LocalNotepadInitializedResult(
                    "notepad_initialized",
                    0),
                LocalNotepadFolderCommand => new LocalNotepadFolderMutationResult(
                    "notepad_folder_mutation",
                    "work/ideas",
                    0),
                LocalNotepadRenameFolderCommand => RenameResponse(),
                LocalNotepadCreateNoteCommand => new LocalNotepadNoteResult(
                    "notepad_note",
                    Detail("Local", "body")),
                LocalNotepadNoteIdentityCommand => GetResponse(),
                LocalNotepadUpdateNoteCommand update => UpdateResponse(update),
                LocalNotepadDeleteNoteCommand => new LocalNotepadNoteDeletedResult(
                    "notepad_note_deleted",
                    "note-1"),
                _ => throw new InvalidOperationException(
                    $"Unexpected notepad command: {typeof(TCommand).Name}"),
            };
            return Task.FromResult((TResponse)response);
        }

        private LocalNotepadFolderMutationResult RenameResponse()
        {
            _renamed = true;
            _version += 1;
            return new("notepad_folder_mutation", "archive", 1);
        }

        private LocalNotepadNoteResult GetResponse()
        {
            if (_renamed) GetCountAfterRename += 1;
            return new("notepad_note", Detail("Local", "body"));
        }

        private LocalNotepadNoteResult UpdateResponse(LocalNotepadUpdateNoteCommand update)
        {
            Assert.Equal(_version, update.ExpectedVersion);
            _version += 1;
            return new("notepad_note", Detail(update.Title ?? "Updated", update.Content ?? "body"));
        }

        private WindowsLocalNotepadNoteDetail Detail(string title, string content) => new(
            new WindowsLocalNotepadNote(
                "note-1",
                "user-1",
                title,
                _renamed ? "archive/ideas" : "work/ideas",
                ["rust"],
                "note-1.md",
                _version,
                1,
                2),
            content);
    }
}
