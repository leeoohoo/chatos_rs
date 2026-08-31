using System.Text;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using Renci.SshNet.Common;

namespace ChatOS.Connector.Remote;

public sealed class SshNetRemoteSftpService : IRemoteSftpService
{
    private const long MaximumTextBytes = 2 * 1024 * 1024;
    private const int MaximumRecursiveEntries = 10_000;
    private readonly IRemoteConnectionRuntime _runtime;
    private readonly IRemoteSshSessionFactory _sessions;

    public SshNetRemoteSftpService(IRemoteConnectionRuntime runtime, IRemoteSshSessionFactory sessions)
    { _runtime = runtime; _sessions = sessions; }

    public async Task<IReadOnlyList<RemoteFileEntry>> ListAsync(string connectionId, string path, string? verificationCode = null, CancellationToken cancellationToken = default)
    {
        using var session = await ConnectAsync(connectionId, verificationCode, cancellationToken).ConfigureAwait(false);
        var entries = new List<RemoteFileEntry>();
        await foreach (var file in session.Client.ListDirectoryAsync(CleanPath(path), cancellationToken).ConfigureAwait(false))
        {
            if (file.Name is "." or "..") continue;
            entries.Add(new RemoteFileEntry(file.Name, file.FullName, file.IsDirectory, file.IsSymbolicLink, file.Length, new DateTimeOffset(file.LastWriteTimeUtc, TimeSpan.Zero)));
            if (entries.Count > MaximumRecursiveEntries) throw new InvalidOperationException("远端目录条目过多，请缩小浏览范围。");
        }
        return entries.OrderByDescending(static value => value.IsDirectory).ThenBy(static value => value.Name, StringComparer.OrdinalIgnoreCase).ToArray();
    }

    public async Task<string> ReadTextAsync(string connectionId, string path, string? verificationCode = null, CancellationToken cancellationToken = default)
    {
        using var session = await ConnectAsync(connectionId, verificationCode, cancellationToken).ConfigureAwait(false);
        var attributes = await session.Client.GetAttributesAsync(CleanPath(path), cancellationToken).ConfigureAwait(false);
        if (attributes.Size > MaximumTextBytes) throw new InvalidOperationException("远端文件超过 2 MB，请下载后查看。");
        await using var output = new MemoryStream((int)Math.Max(0, attributes.Size));
        await session.Client.DownloadFileAsync(CleanPath(path), output, cancellationToken).ConfigureAwait(false);
        try { return new UTF8Encoding(false, true).GetString(output.GetBuffer(), 0, checked((int)output.Length)); }
        catch (DecoderFallbackException exception) { throw new InvalidDataException("远端文件不是 UTF-8 文本，请下载二进制文件。", exception); }
    }

    public async Task DownloadAsync(string connectionId, string remotePath, Stream destination, string? verificationCode = null, CancellationToken cancellationToken = default)
    {
        using var session = await ConnectAsync(connectionId, verificationCode, cancellationToken).ConfigureAwait(false);
        await session.Client.DownloadFileAsync(CleanPath(remotePath), destination, cancellationToken).ConfigureAwait(false);
    }

    public async Task UploadAsync(string connectionId, Stream source, string remotePath, bool overwrite, string? verificationCode = null, CancellationToken cancellationToken = default)
    {
        using var session = await ConnectAsync(connectionId, verificationCode, cancellationToken).ConfigureAwait(false);
        var path = CleanPath(remotePath);
        if (!overwrite)
        {
            try { _ = await session.Client.GetAttributesAsync(path, cancellationToken).ConfigureAwait(false); throw new IOException("远端目标已存在，请确认覆盖后重试。"); }
            catch (SftpPathNotFoundException) { }
        }
        await session.Client.UploadFileAsync(source, path, overwrite, null, cancellationToken).ConfigureAwait(false);
    }

    public async Task CreateDirectoryAsync(string connectionId, string path, string? verificationCode = null, CancellationToken cancellationToken = default)
    {
        using var session = await ConnectAsync(connectionId, verificationCode, cancellationToken).ConfigureAwait(false);
        await session.Client.CreateDirectoryAsync(CleanPath(path), cancellationToken).ConfigureAwait(false);
    }

    public async Task RenameAsync(string connectionId, string sourcePath, string destinationPath, string? verificationCode = null, CancellationToken cancellationToken = default)
    {
        using var session = await ConnectAsync(connectionId, verificationCode, cancellationToken).ConfigureAwait(false);
        await session.Client.RenameFileAsync(CleanPath(sourcePath), CleanPath(destinationPath), cancellationToken).ConfigureAwait(false);
    }

    public async Task DeleteAsync(string connectionId, string path, bool recursive, string? verificationCode = null, CancellationToken cancellationToken = default)
    {
        using var session = await ConnectAsync(connectionId, verificationCode, cancellationToken).ConfigureAwait(false);
        var counter = 0;
        await DeleteCoreAsync(session, CleanPath(path), recursive, () =>
        {
            if (++counter > MaximumRecursiveEntries) throw new InvalidOperationException("递归删除超过安全条目上限，操作已停止。");
        }, cancellationToken).ConfigureAwait(false);
    }

    private async Task<RemoteSftpSession> ConnectAsync(string id, string? code, CancellationToken token) =>
        await _sessions.ConnectSftpAsync(await _runtime.ResolveDraftAsync(id, token).ConfigureAwait(false), code, token).ConfigureAwait(false);

    private static async Task DeleteCoreAsync(RemoteSftpSession session, string path, bool recursive, Action count, CancellationToken token)
    {
        var attributes = await session.Client.GetAttributesAsync(path, token).ConfigureAwait(false);
        count();
        if (attributes.IsSymbolicLink || !attributes.IsDirectory)
        {
            await session.Client.DeleteFileAsync(path, token).ConfigureAwait(false);
            return;
        }
        if (!recursive) { await session.Client.DeleteDirectoryAsync(path, token).ConfigureAwait(false); return; }
        await foreach (var entry in session.Client.ListDirectoryAsync(path, token).ConfigureAwait(false))
        {
            if (entry.Name is "." or "..") continue;
            if (entry.IsSymbolicLink) { count(); await session.Client.DeleteFileAsync(entry.FullName, token).ConfigureAwait(false); }
            else await DeleteCoreAsync(session, entry.FullName, true, count, token).ConfigureAwait(false);
        }
        await session.Client.DeleteDirectoryAsync(path, token).ConfigureAwait(false);
    }

    private static string CleanPath(string path) => string.IsNullOrWhiteSpace(path) ? "." : path.Trim();
}
