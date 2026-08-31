using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IRemoteSftpService
{
    Task<IReadOnlyList<RemoteFileEntry>> ListAsync(string connectionId, string path, string? verificationCode = null, CancellationToken cancellationToken = default);
    Task<string> ReadTextAsync(string connectionId, string path, string? verificationCode = null, CancellationToken cancellationToken = default);
    Task DownloadAsync(string connectionId, string remotePath, Stream destination, string? verificationCode = null, CancellationToken cancellationToken = default);
    Task UploadAsync(string connectionId, Stream source, string remotePath, bool overwrite, string? verificationCode = null, CancellationToken cancellationToken = default);
    Task CreateDirectoryAsync(string connectionId, string path, string? verificationCode = null, CancellationToken cancellationToken = default);
    Task RenameAsync(string connectionId, string sourcePath, string destinationPath, string? verificationCode = null, CancellationToken cancellationToken = default);
    Task DeleteAsync(string connectionId, string path, bool recursive, string? verificationCode = null, CancellationToken cancellationToken = default);
}
