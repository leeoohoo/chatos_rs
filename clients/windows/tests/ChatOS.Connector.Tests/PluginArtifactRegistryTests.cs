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
    public async Task PersistsProjectArtifactAndAddsAuthoritativeDeliveryLink()
    {
        var root = Path.Combine(_directory, "artifacts");
        var project = Path.Combine(_directory, "project");
        Directory.CreateDirectory(root);
        Directory.CreateDirectory(project);
        var bytes = Encoding.UTF8.GetBytes("artifact-content");
        var path = Path.Combine(root, "report.txt");
        await File.WriteAllBytesAsync(path, bytes);
        var registry = new PluginArtifactRegistry();

        var result = await registry.RegisterAsync(
            Identity(),
            "owner-1",
            "device-1",
            project,
            Permissions(),
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
        var relativePath = descriptor.GetProperty("workspace_relative_path").GetString()!;
        Assert.StartsWith("chatos-plugin-artifacts/session-1/pa_", relativePath, StringComparison.Ordinal);
        Assert.Equal(bytes, await File.ReadAllBytesAsync(Path.Combine(
            project, relativePath.Replace('/', Path.DirectorySeparatorChar))));
        var delivery = result.GetProperty("content")[0].GetProperty("text").GetString();
        Assert.Contains($"[report.txt]({relativePath})", delivery, StringComparison.Ordinal);

        await using var destination = new MemoryStream();
        var copied = await registry.CopyToAsync(artifactId, destination);
        Assert.Equal(bytes, destination.ToArray());
        Assert.Equal("generate_report", copied.ProducerToolName);

        await File.WriteAllTextAsync(Path.Combine(
            project, relativePath.Replace('/', Path.DirectorySeparatorChar)), "tampered");
        await Assert.ThrowsAsync<PluginRuntimeException>(() => registry.CopyToAsync(
            artifactId,
            new MemoryStream()));
    }

    [Fact]
    public async Task RejectsTraversalMismatchedHashAndUnsupportedMimeType()
    {
        var root = Path.Combine(_directory, "safe-root");
        var project = Path.Combine(_directory, "project");
        Directory.CreateDirectory(root);
        Directory.CreateDirectory(project);
        var outside = Path.Combine(_directory, "outside.txt");
        var bytes = Encoding.UTF8.GetBytes("outside");
        await File.WriteAllBytesAsync(outside, bytes);
        var registry = new PluginArtifactRegistry();

        await Assert.ThrowsAsync<PluginRuntimeException>(() => registry.RegisterAsync(
            Identity(),
            "owner-1",
            "device-1",
            project,
            Permissions(),
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
            project,
            Permissions(),
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
            project,
            Permissions(),
            root,
            "unsafe",
            Candidate("binary.exe", "binary.exe", "application/octet-stream", bytes),
            CancellationToken.None));
    }

    [Fact]
    public async Task RegistersDiagramAndSolutionArtifactMediaTypes()
    {
        var root = Path.Combine(_directory, "studio-artifacts");
        var project = Path.Combine(_directory, "studio-project");
        Directory.CreateDirectory(root);
        Directory.CreateDirectory(project);
        var registry = new PluginArtifactRegistry();
        var fixtures = new[]
        {
            (Name: "费用报销审批.svg", MediaType: "image/svg+xml", Content: "<svg/>"),
            (Name: "费用报销审批.diagram.json", MediaType: "application/vnd.chatos.diagram+json", Content: "{}"),
            (Name: "费用报销审批.puml", MediaType: "text/vnd.plantuml", Content: "@startuml\n@enduml\n"),
            (Name: "费用报销审批.md", MediaType: "text/markdown", Content: "# 费用报销审批\n"),
        };

        foreach (var fixture in fixtures)
        {
            var bytes = Encoding.UTF8.GetBytes(fixture.Content);
            await File.WriteAllBytesAsync(Path.Combine(root, fixture.Name), bytes);
            var result = await registry.RegisterAsync(
                Identity(),
                "owner-1",
                "device-1",
                project,
                Permissions(),
                root,
                "studio_export",
                Candidate(fixture.Name, fixture.Name, fixture.MediaType, bytes),
                CancellationToken.None);

            var descriptor = result.GetProperty("_meta").GetProperty("chatos/artifacts")[0]
                .GetProperty("artifact");
            Assert.Equal(fixture.Name, descriptor.GetProperty("display_name").GetString());
            Assert.Equal(fixture.MediaType, descriptor.GetProperty("media_type").GetString());
        }
    }

    [Fact]
    public async Task RejectsArtifactWithoutPermissionOrProjectWorkspace()
    {
        var root = Path.Combine(_directory, "permission-artifacts");
        var project = Path.Combine(_directory, "permission-project");
        Directory.CreateDirectory(root);
        Directory.CreateDirectory(project);
        var bytes = Encoding.UTF8.GetBytes("artifact-content");
        await File.WriteAllBytesAsync(Path.Combine(root, "report.txt"), bytes);
        var registry = new PluginArtifactRegistry();

        var denied = await Assert.ThrowsAsync<PluginRuntimeException>(() => registry.RegisterAsync(
            Identity(), "owner-1", "device-1", project, new HashSet<string>(), root,
            "generate_report", Candidate("report.txt", "report.txt", "text/plain", bytes),
            CancellationToken.None));
        Assert.Contains("not allowed", denied.Message, StringComparison.Ordinal);
        Assert.Empty(Directory.EnumerateFileSystemEntries(project));

        var unscoped = await Assert.ThrowsAsync<PluginRuntimeException>(() => registry.RegisterAsync(
            Identity() with { WorkspaceId = null }, "owner-1", "device-1", null, Permissions(), root,
            "generate_report", Candidate("report.txt", "report.txt", "text/plain", bytes),
            CancellationToken.None));
        Assert.Contains("project workspace", unscoped.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task AcceptsLegacyMimeAndComputedIntegrityButRejectsUnknownFields()
    {
        var root = Path.Combine(_directory, "legacy-artifacts");
        var project = Path.Combine(_directory, "legacy-project");
        Directory.CreateDirectory(root);
        Directory.CreateDirectory(project);
        var bytes = Encoding.UTF8.GetBytes("legacy-content");
        await File.WriteAllBytesAsync(Path.Combine(root, "legacy.txt"), bytes);
        var registry = new PluginArtifactRegistry();
        var legacy = JsonSerializer.SerializeToElement(new
        {
            content = Array.Empty<object>(),
            _meta = new Dictionary<string, object>
            {
                ["chatos/artifacts"] = new[]
                {
                    new
                    {
                        producer_artifact_id = "legacy-1",
                        relative_path = "legacy.txt",
                        display_name = "legacy.txt",
                        mime_type = "text/plain",
                    },
                },
            },
        });

        var accepted = await registry.RegisterAsync(
            Identity(), "owner-1", "device-1", project, Permissions(), root,
            "legacy_export", legacy, CancellationToken.None);
        Assert.Equal(Sha256(bytes), accepted.GetProperty("_meta")
            .GetProperty("chatos/artifacts")[0].GetProperty("artifact")
            .GetProperty("sha256").GetString());

        var unknown = JsonSerializer.SerializeToElement(new
        {
            content = Array.Empty<object>(),
            _meta = new Dictionary<string, object>
            {
                ["chatos/artifacts"] = new[]
                {
                    new
                    {
                        producer_artifact_id = "legacy-2",
                        relative_path = "legacy.txt",
                        display_name = "legacy.txt",
                        media_type = "text/plain",
                        ignored = true,
                    },
                },
            },
        });
        await Assert.ThrowsAsync<PluginRuntimeException>(() => registry.RegisterAsync(
            Identity(), "owner-1", "device-1", project, Permissions(), root,
            "legacy_export", unknown, CancellationToken.None));
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

    private static IReadOnlySet<string> Permissions() =>
        new HashSet<string>(["artifact.create"], StringComparer.Ordinal);

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
