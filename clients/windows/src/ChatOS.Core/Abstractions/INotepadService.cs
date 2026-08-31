using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface INotepadService
{
    Task InitializeAsync(CancellationToken cancellationToken = default);

    Task<IReadOnlyList<string>> ListFoldersAsync(CancellationToken cancellationToken = default);

    Task CreateFolderAsync(string folder, CancellationToken cancellationToken = default);

    Task RenameFolderAsync(string from, string to, CancellationToken cancellationToken = default);

    Task DeleteFolderAsync(
        string folder,
        bool recursive,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<NotepadNote>> ListNotesAsync(
        string? query,
        int limit = 500,
        CancellationToken cancellationToken = default);

    Task<NotepadNoteDetail> CreateNoteAsync(
        NotepadNoteDraft draft,
        CancellationToken cancellationToken = default);

    Task<NotepadNoteDetail> FetchNoteAsync(
        string id,
        CancellationToken cancellationToken = default);

    Task<NotepadNoteDetail> UpdateNoteAsync(
        string id,
        NotepadNoteUpdate update,
        CancellationToken cancellationToken = default);

    Task DeleteNoteAsync(string id, CancellationToken cancellationToken = default);
}
