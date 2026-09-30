using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed record WindowsLocalNotepadNote(
    string NoteId,
    string OwnerUserId,
    string Title,
    string Folder,
    IReadOnlyList<string> Tags,
    string File,
    ulong Version,
    long CreatedAtUnixMs,
    long UpdatedAtUnixMs);

public sealed record WindowsLocalNotepadNoteDetail(
    WindowsLocalNotepadNote Note,
    string Content);

internal sealed record LocalNotepadOwnerCommand(string Type, string OwnerUserId);
internal sealed record LocalNotepadFolderCommand(string Type, string OwnerUserId, string Folder);
internal sealed record LocalNotepadRenameFolderCommand(
    string Type,
    string OwnerUserId,
    string From,
    string To);
internal sealed record LocalNotepadDeleteFolderCommand(
    string Type,
    string OwnerUserId,
    string Folder,
    bool Recursive);
internal sealed record LocalNotepadListNotesCommand(
    string Type,
    string OwnerUserId,
    string? Query,
    uint Limit);
internal sealed record LocalNotepadCreateNoteCommand(
    string Type,
    string OwnerUserId,
    string Folder,
    string Title,
    string Content,
    IReadOnlyList<string> Tags);
internal sealed record LocalNotepadNoteIdentityCommand(
    string Type,
    string OwnerUserId,
    string NoteId);
internal sealed record LocalNotepadUpdateNoteCommand(
    string Type,
    string OwnerUserId,
    string NoteId,
    ulong ExpectedVersion,
    string? Title,
    string? Content,
    string? Folder,
    IReadOnlyList<string>? Tags);
internal sealed record LocalNotepadDeleteNoteCommand(
    string Type,
    string OwnerUserId,
    string NoteId,
    ulong ExpectedVersion);

internal sealed record LocalNotepadInitializedResult(string Type, ulong NoteCount);
internal sealed record LocalNotepadFoldersResult(string Type, IReadOnlyList<string> Folders);
internal sealed record LocalNotepadFolderMutationResult(
    string Type,
    string Folder,
    ulong AffectedNotes);
internal sealed record LocalNotepadNotesResult(
    string Type,
    IReadOnlyList<WindowsLocalNotepadNote> Notes);
internal sealed record LocalNotepadNoteResult(
    string Type,
    WindowsLocalNotepadNoteDetail Detail);
internal sealed record LocalNotepadNoteDeletedResult(string Type, string NoteId);

public sealed class WindowsLocalAgentNotepadClient(ILocalAgentHostClient host)
{
    public async Task InitializeAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<
            LocalNotepadOwnerCommand,
            LocalNotepadInitializedResult>(
                new("initialize_notepad", ownerUserId),
                cancellationToken).ConfigureAwait(false);
        RequireType(response.Type, "notepad_initialized");
    }

    public async Task<IReadOnlyList<string>> ListFoldersAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<
            LocalNotepadOwnerCommand,
            LocalNotepadFoldersResult>(
                new("list_notepad_folders", ownerUserId),
                cancellationToken).ConfigureAwait(false);
        RequireType(response.Type, "notepad_folders");
        return response.Folders;
    }

    public async Task CreateFolderAsync(
        string ownerUserId,
        string folder,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<
            LocalNotepadFolderCommand,
            LocalNotepadFolderMutationResult>(
                new("create_notepad_folder", ownerUserId, folder),
                cancellationToken).ConfigureAwait(false);
        RequireType(response.Type, "notepad_folder_mutation");
    }

    public async Task RenameFolderAsync(
        string ownerUserId,
        string from,
        string to,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<
            LocalNotepadRenameFolderCommand,
            LocalNotepadFolderMutationResult>(
                new("rename_notepad_folder", ownerUserId, from, to),
                cancellationToken).ConfigureAwait(false);
        RequireType(response.Type, "notepad_folder_mutation");
    }

    public async Task DeleteFolderAsync(
        string ownerUserId,
        string folder,
        bool recursive,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<
            LocalNotepadDeleteFolderCommand,
            LocalNotepadFolderMutationResult>(
                new("delete_notepad_folder", ownerUserId, folder, recursive),
                cancellationToken).ConfigureAwait(false);
        RequireType(response.Type, "notepad_folder_mutation");
    }

    public async Task<IReadOnlyList<WindowsLocalNotepadNote>> ListNotesAsync(
        string ownerUserId,
        string? query,
        int limit,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<
            LocalNotepadListNotesCommand,
            LocalNotepadNotesResult>(
                new("list_notepad_notes", ownerUserId, query, (uint)Math.Clamp(limit, 1, 500)),
                cancellationToken).ConfigureAwait(false);
        RequireType(response.Type, "notepad_notes");
        return response.Notes;
    }

    public async Task<WindowsLocalNotepadNoteDetail> CreateNoteAsync(
        string ownerUserId,
        NotepadNoteDraft draft,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<
            LocalNotepadCreateNoteCommand,
            LocalNotepadNoteResult>(
                new(
                    "create_notepad_note",
                    ownerUserId,
                    draft.Folder,
                    draft.Title,
                    draft.Content,
                    draft.Tags),
                cancellationToken).ConfigureAwait(false);
        return RequireNote(response);
    }

    public async Task<WindowsLocalNotepadNoteDetail> GetNoteAsync(
        string ownerUserId,
        string noteId,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<
            LocalNotepadNoteIdentityCommand,
            LocalNotepadNoteResult>(
                new("get_notepad_note", ownerUserId, noteId),
                cancellationToken).ConfigureAwait(false);
        return RequireNote(response);
    }

    public async Task<WindowsLocalNotepadNoteDetail> UpdateNoteAsync(
        string ownerUserId,
        string noteId,
        ulong expectedVersion,
        NotepadNoteUpdate update,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<
            LocalNotepadUpdateNoteCommand,
            LocalNotepadNoteResult>(
                new(
                    "update_notepad_note",
                    ownerUserId,
                    noteId,
                    expectedVersion,
                    update.Title,
                    update.Content,
                    update.Folder,
                    update.Tags),
                cancellationToken).ConfigureAwait(false);
        return RequireNote(response);
    }

    public async Task DeleteNoteAsync(
        string ownerUserId,
        string noteId,
        ulong expectedVersion,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<
            LocalNotepadDeleteNoteCommand,
            LocalNotepadNoteDeletedResult>(
                new("delete_notepad_note", ownerUserId, noteId, expectedVersion),
                cancellationToken).ConfigureAwait(false);
        RequireType(response.Type, "notepad_note_deleted");
    }

    private static WindowsLocalNotepadNoteDetail RequireNote(LocalNotepadNoteResult response)
    {
        RequireType(response.Type, "notepad_note");
        return response.Detail;
    }

    private static void RequireType(string actual, string expected)
    {
        if (!string.Equals(actual, expected, StringComparison.Ordinal))
        {
            throw new InvalidDataException("Local Agent Host returned invalid notepad data.");
        }
    }
}
