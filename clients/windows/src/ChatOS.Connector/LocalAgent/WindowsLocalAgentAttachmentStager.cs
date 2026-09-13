using System.Security.Cryptography;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentAttachmentStager
{
    public const int MaximumAttachmentBytes = 5 * 1024 * 1024;
    public const int MaximumTotalBytes = 6 * 1024 * 1024;

    public async Task<IReadOnlyList<LocalAgentAttachmentReference>> StageAsync(
        IReadOnlyList<ConversationAttachmentDraft> attachments,
        string grantDirectory,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(attachments);
        WindowsLocalAgentHostBootstrapBuilder.EnsurePrivateDirectory(grantDirectory);
        Validate(attachments);
        var paths = new List<string>();
        try
        {
            var result = new List<LocalAgentAttachmentReference>(attachments.Count);
            foreach (var attachment in attachments)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var grantId = $"grant-{Guid.NewGuid():D}";
                var path = Path.Combine(grantDirectory, $"{grantId}.payload");
                await using (var stream = new FileStream(
                    path,
                    FileMode.CreateNew,
                    FileAccess.Write,
                    FileShare.None,
                    64 * 1024,
                    FileOptions.Asynchronous | FileOptions.WriteThrough))
                {
                    await stream.WriteAsync(attachment.Data, cancellationToken).ConfigureAwait(false);
                    await stream.FlushAsync(cancellationToken).ConfigureAwait(false);
                }
                paths.Add(path);
                result.Add(new LocalAgentAttachmentReference(
                    attachment.Id,
                    attachment.MimeType,
                    $"attachment-grant:{grantId}",
                    $"sha256:{Convert.ToHexString(SHA256.HashData(attachment.Data)).ToLowerInvariant()}",
                    checked((ulong)attachment.Data.Length)));
            }
            return result;
        }
        catch
        {
            foreach (var path in paths) DeleteExact(path);
            throw;
        }
    }

    public void Discard(
        IReadOnlyList<LocalAgentAttachmentReference> references,
        string grantDirectory)
    {
        ArgumentNullException.ThrowIfNull(references);
        foreach (var reference in references)
        {
            const string prefix = "attachment-grant:grant-";
            if (!reference.PayloadReference.StartsWith(prefix, StringComparison.Ordinal)) continue;
            var value = reference.PayloadReference[prefix.Length..];
            if (!Guid.TryParseExact(value, "D", out var parsed)
                || value != value.ToLowerInvariant())
            {
                continue;
            }
            var grantId = $"grant-{parsed:D}";
            DeleteExact(Path.Combine(grantDirectory, $"{grantId}.payload"));
        }
    }

    private static void Validate(IReadOnlyList<ConversationAttachmentDraft> attachments)
    {
        var ids = new HashSet<string>(StringComparer.Ordinal);
        long total = 0;
        foreach (var attachment in attachments)
        {
            if (string.IsNullOrWhiteSpace(attachment.Id)
                || attachment.Id != attachment.Id.Trim()
                || attachment.Id.Any(char.IsControl)
                || !ids.Add(attachment.Id)
                || string.IsNullOrWhiteSpace(attachment.MimeType)
                || attachment.MimeType.Any(char.IsWhiteSpace)
                || !attachment.MimeType.Contains('/')
                || attachment.Data.Length == 0)
            {
                throw new InvalidDataException($"Attachment '{attachment.Name}' is invalid.");
            }
            if (attachment.Data.Length > MaximumAttachmentBytes)
            {
                throw new InvalidDataException($"Attachment '{attachment.Name}' exceeds 5 MB.");
            }
            total = checked(total + attachment.Data.Length);
            if (total > MaximumTotalBytes)
            {
                throw new InvalidDataException("Main Chat attachments exceed 6 MB in total.");
            }
        }
    }

    private static void DeleteExact(string path)
    {
        try
        {
            if (File.Exists(path)) File.Delete(path);
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }
}
