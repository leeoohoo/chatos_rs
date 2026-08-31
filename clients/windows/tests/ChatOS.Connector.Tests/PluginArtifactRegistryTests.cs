using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Plugins;

namespace ChatOS.Connector.Tests;

public sealed class PluginArtifactRegistryTests : IDisposable
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(),
        $"chatos-plugin-artifacts-{Guid.NewGuid():N}");

    [Fact]
    public async Task RegistersAuthoritativeDescriptorAndCopiesOnlyUnchangedFile()
    {
        var root = Path.Combine(_directory, "artifacts");
        Directory.CreateDirectory(root);
        var bytes = Encoding.UTF8.GetBytes("artifact-content");
        var path = Path.Combine(root, "report.txt");
        await File.WriteAllBytesAsync(path, bytes);
        var registry = new PluginArtifactRegistry();

        var result = await registry.RegisterAsync(
            Identity(),
            "owner-1",
            "device-1",
            root,
            "generate_report",
            Candidate("report.txt", "report.txt", "text/plain", bytes),
            CancellationToken.None);

        var authoritative = result.GetProperty("_meta").GetProperty("chatos/artifacts")[0];
        var descriptor = authoritative.GetProperty("artifact");
        var artifactId = descriptor.GetProperty("artifact_id").GetString()!;
        Assert.StartsWith("pa_", artifactId, StringComparison.Ordinal);
        Assert.Equal("workspace-1", descriptor.GetProperty("owner").GetProperty("workspace_id").GetString());
        Assert.Equal(Sha256(bytes), descriptor.GetProperty("sha256").GetString());
        Assert.Equal("producer-1", authoritative.GetProperty("producer_artifact_id").GetString());

        await using var destination = new MemoryStream();
        var copied = await registry.CopyToAsync(artifactId, destination);
        Assert.Equal(bytes, destination.ToArray());
        Assert.Equal("generate_report", copied.ProducerToolName);

        await File.WriteAllTextAsync(path, "tampered");
        await Assert.ThrowsAsync<PluginRuntimeException>(() => registry.CopyToAsync(
            artifactId,
            new MemoryStream()));
    }

    [Fact]
    public async Task RejectsTraversalMismatchedHashAndUnsupportedMimeType()
    {
        var root = Path.Combine(_directory, "safe-root");
        Directory.CreateDirectory(root);
        var outside = Path.Combine(_directory, "outside.txt");
        var bytes = Encoding.UTF8.GetBytes("outside");
        await File.WriteAllBytesAsync(outside, bytes);
        var registry = new PluginArtifactRegistry();

        await Assert.ThrowsAsync<PluginRuntimeException>(() => registry.RegisterAsync(
            Identity(),
            "owner-1",
            "device-1",
            root,
            "unsafe",
            Candidate("../outside.txt", "outside.txt", "text/plain", bytes),
            CancellationToken.None));

        var local = Path.Combine(root, "capture.png");
        await File.WriteAllBytesAsync(local, bytes);
        var badHash = Candidate("capture.png", "capture.png", "image/png", bytes, new string('a', 64));
        await Assert.ThrowsAsync<PluginRuntimeException>(() => registry.RegisterAsync(
            Identity(),
            "owner-1",
            "device-1",
            root,
            "unsafe",
            badHash,
            CancellationToken.None));

        var unsupported = Path.Combine(root, "binary.exe");
        await File.WriteAllBytesAsync(unsupported, bytes);
        await Assert.ThrowsAsync<PluginRuntimeException>(() => registry.RegisterAsync(
            Identity(),
            "owner-1",
            "device-1",
            root,
            "unsafe",
            Candidate("binary.exe", "binary.exe", "application/octet-stream", bytes),
            CancellationToken.None));
    }

    public void Dispose()
    {
        try
        {
            if (Directory.Exists(_directory))
            {
                Directory.Delete(_directory, recursive: true);
            }
        }
        catch (IOException)
        {
        }
    }

    private static PluginRuntimeIdentity Identity() => new(
        "run-1",
        "plugin-1",
        "release-1",
        "1.0.0",
        new string('a', 64),
        "main",
        "session-1",
        "workspace-1");

    private static JsonElement Candidate(
        string relativePath,
        string displayName,
        string mediaType,
        byte[] bytes,
        string? sha256 = null) => JsonSerializer.SerializeToElement(new
    {
        content = new { ok = true },
        _meta = new Dictionary<string, object>
        {
            ["chatos/artifacts"] = new[]
            {
                new
                {
                    producer_artifact_id = "producer-1",
                    relative_path = relativePath,
                    display_name = displayName,
                    media_type = mediaType,
                    size_bytes = bytes.LongLength,
                    sha256 = sha256 ?? Sha256(bytes),
                },
            },
        },
    });

    private static string Sha256(byte[] value) =>
        Convert.ToHexString(SHA256.HashData(value)).ToLowerInvariant();
}
