using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentAttachmentVaultTests : IDisposable
{
    private readonly string _root = Path.Combine(
        Path.GetTempPath(),
        "chatos-windows-attachment-test-" + Guid.NewGuid().ToString("N"));

    [Fact]
    public void DisplayUriOnlyExposesTheValidatedConversationScopedFile()
    {
        var vault = new WindowsLocalAgentAttachmentVault(_root);
        var spec = Assert.Single(vault.Authorize(
            [
                ConversationAttachmentDraft.Create(
                    "preview.png",
                    "image/png",
                    ConversationAttachmentKind.Image,
                    ConversationAttachmentOrigin.File,
                    [1, 2, 3, 4]),
            ],
            "owner",
            "conversation"));
        var record = new WindowsLocalConversationAttachmentRecord(
            spec.AttachmentId,
            "conversation",
            "turn",
            "message",
            1,
            spec.DisplayName,
            spec.MediaType,
            spec.ByteSize,
            spec.Sha256,
            spec.AuthorizedLocalRef,
            JsonSerializer.SerializeToElement(new { }),
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());

        var uri = vault.ResolveDisplayUri(record, "owner", "conversation");

        Assert.NotNull(uri);
        Assert.True(uri!.IsFile);
        Assert.True(File.Exists(uri.LocalPath));
        Assert.Null(vault.ResolveDisplayUri(record, "another-owner", "conversation"));
    }

    public void Dispose()
    {
        if (Directory.Exists(_root))
        {
            Directory.Delete(_root, true);
        }
    }
}
