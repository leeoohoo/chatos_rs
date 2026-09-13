using System.Security.Cryptography;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentAttachmentStagerTests : IDisposable
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(), $"chatos-main-chat-grants-{Guid.NewGuid():N}");
    private readonly WindowsLocalAgentAttachmentStager _stager = new();

    [Fact]
    public async Task StagesOpaqueGrantWithExactBytesSizeMimeAndDigest()
    {
        var bytes = new byte[] { 1, 4, 9, 16 };
        var draft = Draft("attachment-1", "image/png", bytes);

        var reference = Assert.Single(await _stager.StageAsync([draft], _directory));

        Assert.Equal("attachment-1", reference.AttachmentId);
        Assert.Equal("image/png", reference.MediaType);
        Assert.Equal((ulong)bytes.Length, reference.ByteSize);
        Assert.StartsWith("attachment-grant:grant-", reference.PayloadReference);
        Assert.Equal($"sha256:{Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant()}",
            reference.PayloadDigest);
        var path = GrantPath(reference);
        Assert.Equal(bytes, await File.ReadAllBytesAsync(path));
        Assert.DoesNotContain(_directory, reference.PayloadReference, StringComparison.Ordinal);
    }

    [Fact]
    public async Task DiscardDeletesOnlyAValidatedExactGrantReference()
    {
        var reference = Assert.Single(await _stager.StageAsync(
            [Draft("attachment-1", "text/plain", [1])], _directory));
        var path = GrantPath(reference);
        var unrelated = Path.Combine(_directory, "keep.payload");
        await File.WriteAllBytesAsync(unrelated, [2]);

        _stager.Discard([
            reference,
            reference with { PayloadReference = "attachment-grant:../../keep" },
        ], _directory);

        Assert.False(File.Exists(path));
        Assert.True(File.Exists(unrelated));
    }

    [Fact]
    public async Task RejectsDuplicateIdentityAndHostSizeLimitsBeforeWritingAnything()
    {
        var duplicate = new[] {
            Draft("same", "text/plain", [1]),
            Draft("same", "text/plain", [2]),
        };
        await Assert.ThrowsAsync<InvalidDataException>(() => _stager.StageAsync(duplicate, _directory));
        Assert.Empty(Directory.EnumerateFiles(_directory));

        var tooLarge = Draft("large", "application/octet-stream",
            new byte[WindowsLocalAgentAttachmentStager.MaximumAttachmentBytes + 1]);
        await Assert.ThrowsAsync<InvalidDataException>(() => _stager.StageAsync([tooLarge], _directory));
        Assert.Empty(Directory.EnumerateFiles(_directory));
    }

    private string GrantPath(LocalAgentAttachmentReference reference) => Path.Combine(
        _directory,
        $"{reference.PayloadReference["attachment-grant:".Length..]}.payload");

    private static ConversationAttachmentDraft Draft(string id, string mime, byte[] bytes) => new(
        id, $"{id}.bin", mime, ConversationAttachmentKind.File,
        ConversationAttachmentOrigin.File, bytes);

    public void Dispose()
    {
        if (Directory.Exists(_directory)) Directory.Delete(_directory, recursive: true);
    }
}
