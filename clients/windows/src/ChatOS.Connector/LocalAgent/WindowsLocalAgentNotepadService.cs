using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentNotepadService(
    WindowsLocalAgentNotepadClient client) : INotepadService
{
    private readonly object _gate = new();
    private readonly Dictionary<string, ulong> _versions = new(StringComparer.Ordinal);
    private string? _ownerUserId;

    public void Configure(string ownerUserId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(ownerUserId);
        lock (_gate)
        {
            _ownerUserId = ownerUserId;
            _versions.Clear();
        }
    }

    public void Reset()
    {
        lock (_gate)
        {
            _ownerUserId = null;
            _versions.Clear();
        }
    }

    public Task InitializeAsync(CancellationToken cancellationToken = default) =>
        client.InitializeAsync(Owner(), cancellationToken);

    public Task<IReadOnlyList<string>> ListFoldersAsync(
        CancellationToken cancellationToken = default) =>
        client.ListFoldersAsync(Owner(), cancellationToken);

    public Task CreateFolderAsync(
        string folder,
        CancellationToken cancellationToken = default) =>
        client.CreateFolderAsync(Owner(), folder, cancellationToken);

    public async Task RenameFolderAsync(
        string from,
        string to,
        CancellationToken cancellationToken = default)
    {
        await client.RenameFolderAsync(Owner(), from, to, cancellationToken).ConfigureAwait(false);
        ClearVersions();
    }

    public async Task DeleteFolderAsync(
        string folder,
        bool recursive,
        CancellationToken cancellationToken = default)
    {
        await client.DeleteFolderAsync(Owner(), folder, recursive, cancellationToken)
            .ConfigureAwait(false);
        ClearVersions();
    }

    public async Task<IReadOnlyList<NotepadNote>> ListNotesAsync(
        string? query,
        int limit = 500,
        CancellationToken cancellationToken = default)
    {
        var records = await client.ListNotesAsync(Owner(), query, limit, cancellationToken)
            .ConfigureAwait(false);
        lock (_gate)
        {
            foreach (var record in records) _versions[record.NoteId] = record.Version;
        }
        return records.Select(ToDomain).ToArray();
    }

    public async Task<NotepadNoteDetail> CreateNoteAsync(
        NotepadNoteDraft draft,
        CancellationToken cancellationToken = default) =>
        Cache(await client.CreateNoteAsync(Owner(), draft, cancellationToken).ConfigureAwait(false));

    public async Task<NotepadNoteDetail> FetchNoteAsync(
        string id,
        CancellationToken cancellationToken = default) =>
        Cache(await client.GetNoteAsync(Owner(), id, cancellationToken).ConfigureAwait(false));

    public async Task<NotepadNoteDetail> UpdateNoteAsync(
        string id,
        NotepadNoteUpdate update,
        CancellationToken cancellationToken = default)
    {
        var owner = Owner();
        var version = await VersionAsync(owner, id, cancellationToken).ConfigureAwait(false);
        return Cache(await client.UpdateNoteAsync(
            owner,
            id,
            version,
            update,
            cancellationToken).ConfigureAwait(false));
    }

    public async Task DeleteNoteAsync(
        string id,
        CancellationToken cancellationToken = default)
    {
        var owner = Owner();
        var version = await VersionAsync(owner, id, cancellationToken).ConfigureAwait(false);
        await client.DeleteNoteAsync(owner, id, version, cancellationToken).ConfigureAwait(false);
        lock (_gate) _versions.Remove(id);
    }

    private async Task<ulong> VersionAsync(
        string ownerUserId,
        string noteId,
        CancellationToken cancellationToken)
    {
        lock (_gate)
        {
            if (_versions.TryGetValue(noteId, out var version)) return version;
        }
        var detail = await client.GetNoteAsync(ownerUserId, noteId, cancellationToken)
            .ConfigureAwait(false);
        lock (_gate) _versions[noteId] = detail.Note.Version;
        return detail.Note.Version;
    }

    private NotepadNoteDetail Cache(WindowsLocalNotepadNoteDetail detail)
    {
        lock (_gate) _versions[detail.Note.NoteId] = detail.Note.Version;
        return new NotepadNoteDetail(ToDomain(detail.Note), detail.Content);
    }

    private string Owner()
    {
        lock (_gate)
        {
            return _ownerUserId ?? throw new InvalidOperationException(
                "Local notepad is not configured for an account.");
        }
    }

    private void ClearVersions()
    {
        lock (_gate) _versions.Clear();
    }

    private static NotepadNote ToDomain(WindowsLocalNotepadNote note) => new(
        note.NoteId,
        note.Title,
        note.Folder,
        note.Tags,
        DateTimeOffset.FromUnixTimeMilliseconds(note.CreatedAtUnixMs),
        DateTimeOffset.FromUnixTimeMilliseconds(note.UpdatedAtUnixMs),
        note.File);
}
