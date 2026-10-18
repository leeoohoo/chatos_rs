using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.Plugins;

public enum LocalPluginSourceKind
{
    LocalFolder,
    GitRepository,
    NpmPackage,
}

public sealed record LocalPluginSourceDraft(
    LocalPluginSourceKind Kind,
    string Location,
    string? ExactVersion = null,
    string? GitReference = null,
    string? Subdirectory = null);

public sealed record NpmMcpDraft(
    string DisplayName,
    string Description,
    string ComponentKey,
    string PackageName,
    string PackageVersion,
    string BinaryName,
    IReadOnlyList<string> Arguments);

public enum LocalPluginInspectionComponentKind
{
    Mcp,
    Skill,
}

public sealed record LocalPluginInspectionComponent(
    string ComponentKey,
    string DisplayName,
    LocalPluginInspectionComponentKind Kind,
    IReadOnlyList<string> ToolNames);

public sealed record LocalPluginInspection(
    string Fingerprint,
    string DisplayName,
    string Version,
    string SourceSummary,
    IReadOnlyList<LocalPluginInspectionComponent> Components);

public interface ILocalPluginSourceInstaller
{
    Task<LocalPluginInspection> InspectAsync(
        LocalPluginSourceDraft source,
        CancellationToken cancellationToken = default);

    Task<InstalledPluginRecord> InstallAsync(
        LocalPluginSourceDraft source,
        string expectedFingerprint,
        CancellationToken cancellationToken = default);

    Task<LocalPluginInspection> InspectNpmMcpAsync(
        NpmMcpDraft draft,
        CancellationToken cancellationToken = default);

    Task<InstalledPluginRecord> InstallNpmMcpAsync(
        NpmMcpDraft draft,
        string expectedFingerprint,
        CancellationToken cancellationToken = default);
}

internal interface INpmMcpPackageAcquirer
{
    Task AcquireAsync(
        NpmMcpDraft draft,
        string packageRoot,
        CancellationToken cancellationToken);
}

internal interface IGitPluginSourceAcquirer
{
    Task AcquireAsync(
        string repositoryUrl,
        string reference,
        string destinationRoot,
        CancellationToken cancellationToken);
}

internal sealed record PreparedLocalPlugin(
    InstalledPluginRecord Record,
    PluginManifest Manifest,
    string StagingRoot,
    string PackageRoot,
    string SourceSummary);

/// <summary>
/// Installs user-selected Plugin sources through the same inspect, fingerprint and
/// runtime path used by managed Plugins. Local ChatOS packages and the compatible
/// Agent Plugin, Skill and stdio MCP layouts are normalized only inside staging.
/// Other source adapters must enter through this contract instead of bypassing the
/// preflight fingerprint.
/// </summary>
public sealed class WindowsCustomPluginInstaller : ILocalPluginSourceInstaller
{
    private const int MaximumEntries = 8_192;
    private const int MaximumNpmEntries = 65_536;
    private const long MaximumFileBytes = 128L * 1024 * 1024;
    private const long MaximumPackageBytes = 768L * 1024 * 1024;
    private readonly string _rootPath;
    private readonly PluginManifestLoader _manifestLoader;
    private readonly IPluginMcpClientFactory _clientFactory;
    private readonly IInstalledPluginStore _store;
    private readonly INpmMcpPackageAcquirer _npmAcquirer;
    private readonly IGitPluginSourceAcquirer _gitAcquirer;

    internal WindowsCustomPluginInstaller(
        string rootPath,
        PluginManifestLoader manifestLoader,
        IPluginMcpClientFactory clientFactory,
        IInstalledPluginStore store,
        INpmMcpPackageAcquirer? npmAcquirer = null,
        IGitPluginSourceAcquirer? gitAcquirer = null)
    {
        _rootPath = Path.GetFullPath(rootPath);
        _manifestLoader = manifestLoader;
        _clientFactory = clientFactory;
        _store = store;
        _npmAcquirer = npmAcquirer ?? new WindowsNpmMcpPackageAcquirer();
        _gitAcquirer = gitAcquirer ?? new WindowsGitPluginSourceAcquirer();
    }

    internal WindowsCustomPluginInstaller(
        PluginManifestLoader manifestLoader,
        IPluginMcpClientFactory clientFactory,
        IInstalledPluginStore store)
        : this(Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "ChatOS",
            "WindowsClient",
            "Plugins"), manifestLoader, clientFactory, store)
    {
    }

    public async Task<LocalPluginInspection> InspectAsync(
        LocalPluginSourceDraft source,
        CancellationToken cancellationToken = default)
    {
        var prepared = await PrepareAsync(source, cancellationToken).ConfigureAwait(false);
        try
        {
            return await InspectPreparedAsync(prepared, cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            DeleteDirectory(prepared.StagingRoot);
        }
    }

    public async Task<InstalledPluginRecord> InstallAsync(
        LocalPluginSourceDraft source,
        string expectedFingerprint,
        CancellationToken cancellationToken = default)
    {
        if (!IsSha256(expectedFingerprint))
        {
            throw new PluginPackageException("The preflight fingerprint must be a SHA-256 value.");
        }

        // Re-acquire the source instead of committing the preflight copy. This is the
        // source-of-truth drift check required by the cross-platform installer contract.
        var prepared = await PrepareAsync(source, cancellationToken).ConfigureAwait(false);
        try
        {
            var inspection = await InspectPreparedAsync(prepared, cancellationToken).ConfigureAwait(false);
            if (!CryptographicOperations.FixedTimeEquals(
                    Convert.FromHexString(expectedFingerprint),
                    Convert.FromHexString(inspection.Fingerprint)))
            {
                throw new PluginPackageException(
                    "The Plugin source or tool list changed after preflight. Inspect it again before installing.");
            }

            return await CommitAsync(prepared, cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            DeleteDirectory(prepared.StagingRoot);
        }
    }

    public async Task<LocalPluginInspection> InspectNpmMcpAsync(
        NpmMcpDraft draft,
        CancellationToken cancellationToken = default)
    {
        var prepared = await PrepareNpmMcpAsync(draft, cancellationToken).ConfigureAwait(false);
        try
        {
            return await InspectPreparedAsync(prepared, cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            DeleteDirectory(prepared.StagingRoot);
        }
    }

    public async Task<InstalledPluginRecord> InstallNpmMcpAsync(
        NpmMcpDraft draft,
        string expectedFingerprint,
        CancellationToken cancellationToken = default)
    {
        if (!IsSha256(expectedFingerprint))
        {
            throw new PluginPackageException("The preflight fingerprint must be a SHA-256 value.");
        }

        var prepared = await PrepareNpmMcpAsync(draft, cancellationToken).ConfigureAwait(false);
        try
        {
            var inspection = await InspectPreparedAsync(prepared, cancellationToken).ConfigureAwait(false);
            if (!CryptographicOperations.FixedTimeEquals(
                    Convert.FromHexString(expectedFingerprint),
                    Convert.FromHexString(inspection.Fingerprint)))
            {
                throw new PluginPackageException(
                    "The npm package or tool list changed after preflight. Inspect it again before installing.");
            }

            return await CommitAsync(prepared, cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            DeleteDirectory(prepared.StagingRoot);
        }
    }

    private async Task<PreparedLocalPlugin> PrepareNpmMcpAsync(
        NpmMcpDraft draft,
        CancellationToken cancellationToken)
    {
        ValidateNpmMcpDraft(draft);
        Directory.CreateDirectory(_rootPath);
        var stagingRoot = Path.Combine(_rootPath, ".custom-staging", Guid.NewGuid().ToString("N"));
        var packageRoot = Path.Combine(stagingRoot, "package");
        Directory.CreateDirectory(packageRoot);
        try
        {
            await WriteJsonAsync(Path.Combine(packageRoot, "package.json"), new
            {
                name = "chatos-custom-mcp-host",
                version = "1.0.0",
                @private = true,
                dependencies = new Dictionary<string, string>(StringComparer.Ordinal)
                {
                    [draft.PackageName] = draft.PackageVersion,
                },
            }, cancellationToken).ConfigureAwait(false);

            await _npmAcquirer.AcquireAsync(draft, packageRoot, cancellationToken)
                .ConfigureAwait(false);

            var dependencyRoot = Path.GetFullPath(Path.Combine(
                packageRoot, "node_modules", draft.PackageName.Replace('/', Path.DirectorySeparatorChar)));
            if (!IsSameOrChild(packageRoot, dependencyRoot) || !Directory.Exists(dependencyRoot))
            {
                throw new PluginPackageException("npm did not install the pinned package inside staging.");
            }
            RejectReparsePoint(dependencyRoot);
            using var dependencyDocument = JsonDocument.Parse(await File.ReadAllBytesAsync(
                Path.Combine(dependencyRoot, "package.json"), cancellationToken).ConfigureAwait(false));
            var dependency = dependencyDocument.RootElement;
            if (!dependency.TryGetProperty("name", out var installedName) ||
                !string.Equals(installedName.GetString(), draft.PackageName, StringComparison.Ordinal) ||
                !dependency.TryGetProperty("version", out var installedVersion) ||
                !string.Equals(installedVersion.GetString(), draft.PackageVersion, StringComparison.Ordinal))
            {
                throw new PluginPackageException("npm installed package identity does not match the pinned request.");
            }
            var relativeEntry = ResolveNpmBinary(dependency, draft.BinaryName);
            var entryPath = Path.GetFullPath(Path.Combine(dependencyRoot, relativeEntry));
            if (!IsSameOrChild(dependencyRoot, entryPath) || !File.Exists(entryPath))
            {
                throw new PluginPackageException("The pinned npm package does not publish the requested executable.");
            }
            RejectReparsePoint(entryPath);

            var rootEntry = NormalizeRelativePath(Path.GetRelativePath(packageRoot, entryPath));
            await WriteJsonAsync(Path.Combine(packageRoot, "package.json"), new
            {
                name = "chatos-custom-mcp-host",
                version = "1.0.0",
                @private = true,
                dependencies = new Dictionary<string, string>(StringComparer.Ordinal)
                {
                    [draft.PackageName] = draft.PackageVersion,
                },
                bin = new Dictionary<string, string>(StringComparer.Ordinal)
                {
                    [draft.ComponentKey] = rootEntry,
                },
            }, cancellationToken).ConfigureAwait(false);

            var manifest = new PluginManifest
            {
                SchemaVersion = 3,
                Name = draft.PackageName,
                Version = draft.PackageVersion,
                Description = draft.Description,
                Interface = new PluginInterface { DisplayName = draft.DisplayName },
                McpServers = new Dictionary<string, PluginMcpServer>(StringComparer.Ordinal)
                {
                    [draft.ComponentKey] = new()
                    {
                        Type = "stdio",
                        Bin = draft.ComponentKey,
                        Arguments = draft.Arguments.ToArray(),
                        Agent = new PluginAgentPresentation
                        {
                            DisplayName = draft.DisplayName,
                            Description = draft.Description,
                        },
                    },
                },
                Permissions =
                [
                    new PluginPermission
                    {
                        Permission = "process.spawn",
                        Required = true,
                        Reason = "Launch the user-confirmed pinned npm MCP server.",
                        Components = [draft.ComponentKey],
                    },
                ],
            };
            await WriteJsonAsync(
                Path.Combine(packageRoot, "chatos.plugin.json"), manifest, cancellationToken)
                .ConfigureAwait(false);
            ValidateManifest(manifest);
            var hashes = await SnapshotVerifiedTreeAsync(
                    packageRoot, cancellationToken, MaximumNpmEntries)
                .ConfigureAwait(false);
            var artifactSha256 = HashCanonicalFiles(hashes);
            var sourceSummary = $"npm {draft.PackageName}@{draft.PackageVersion} · " +
                $"{draft.BinaryName} {string.Join(' ', draft.Arguments)}".TrimEnd();
            var record = new InstalledPluginRecord(
                $"custom-{Sha256Hex($"npm:{draft.PackageName}:{draft.ComponentKey}")}",
                $"custom:{artifactSha256}",
                draft.PackageVersion,
                artifactSha256,
                packageRoot,
                DateTimeOffset.UtcNow,
                ["process.spawn"],
                hashes,
                draft.PackageName,
                "customMcp",
                sourceSummary,
                true);
            return new PreparedLocalPlugin(record, manifest, stagingRoot, packageRoot, sourceSummary);
        }
        catch
        {
            DeleteDirectory(stagingRoot);
            throw;
        }
    }

    private async Task<PreparedLocalPlugin> PrepareAsync(
        LocalPluginSourceDraft source,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(source);
        var input = ValidatePluginSource(source);

        Directory.CreateDirectory(_rootPath);
        var stagingRoot = Path.Combine(_rootPath, ".custom-staging", Guid.NewGuid().ToString("N"));
        var packageRoot = Path.Combine(stagingRoot, "package");
        try
        {
            string selectedRoot;
            string identity;
            string sourceKind;
            string sourceSummary;
            if (input.Kind is LocalPluginSourceKind.LocalFolder)
            {
                selectedRoot = input.Location;
                if (IsSameOrChild(_rootPath, selectedRoot) || IsSameOrChild(selectedRoot, _rootPath))
                {
                    throw new PluginPackageException(
                        "The Plugin source and ChatOS installation directories must be separate.");
                }
                identity = $"folder:{selectedRoot}";
                sourceKind = "localFolder";
                sourceSummary = $"Local folder · {selectedRoot}";
            }
            else
            {
                var acquiredRoot = Path.Combine(stagingRoot, "acquired");
                Directory.CreateDirectory(acquiredRoot);
                var reference = input.GitReference ?? "HEAD";
                await _gitAcquirer.AcquireAsync(
                    input.Location, reference, acquiredRoot, cancellationToken).ConfigureAwait(false);
                var gitMetadata = Path.Combine(acquiredRoot, ".git");
                DeleteDirectory(gitMetadata);
                if (Directory.Exists(gitMetadata) || File.Exists(gitMetadata))
                {
                    throw new PluginPackageException("Git metadata could not be removed from the isolated source.");
                }
                selectedRoot = SelectPluginRoot(acquiredRoot, input.Subdirectory);
                identity = $"git:{input.Location}:{reference}:{input.Subdirectory ?? string.Empty}";
                sourceKind = "gitRepository";
                sourceSummary = $"Git · {input.Location} · {reference}" +
                    (input.Subdirectory is null ? string.Empty : $" · {input.Subdirectory}");
            }

            Directory.CreateDirectory(packageRoot);
            _ = await CopyVerifiedTreeAsync(
                selectedRoot, packageRoot, cancellationToken).ConfigureAwait(false);
            var imported = WindowsAgentPluginCompatibilityImporter.NormalizePackage(packageRoot);
            var manifest = imported.Manifest;
            ValidateManifest(manifest);
            var hashes = await SnapshotVerifiedTreeAsync(packageRoot, cancellationToken)
                .ConfigureAwait(false);
            var artifactSha256 = HashCanonicalFiles(hashes);
            var pluginId = $"custom-{Sha256Hex($"{identity}:{manifest.Name}")}";
            var record = new InstalledPluginRecord(
                pluginId,
                $"custom:{artifactSha256}",
                manifest.Version.Trim(),
                artifactSha256,
                packageRoot,
                DateTimeOffset.UtcNow,
                manifest.Permissions.Select(value => value.Permission)
                    .Append("process.spawn")
                    .Distinct(StringComparer.Ordinal)
                    .Order(StringComparer.Ordinal)
                    .ToArray(),
                hashes,
                manifest.Name.Trim(),
                sourceKind,
                $"{imported.FormatName} · {sourceSummary}",
                true);
            return new PreparedLocalPlugin(
                record,
                manifest,
                stagingRoot,
                packageRoot,
                record.SourceSummary!);
        }
        catch
        {
            DeleteDirectory(stagingRoot);
            throw;
        }
    }

    private static LocalPluginSourceDraft ValidatePluginSource(LocalPluginSourceDraft source)
    {
        var location = source.Location?.Trim() ?? string.Empty;
        var exactVersion = NullIfWhiteSpace(source.ExactVersion);
        var gitReference = NullIfWhiteSpace(source.GitReference);
        var subdirectory = NullIfWhiteSpace(source.Subdirectory);
        if (location.Length == 0 || location.Length > 4_096 || location.IndexOf('\0') >= 0)
        {
            throw new PluginPackageException("The Plugin source is invalid.");
        }

        switch (source.Kind)
        {
            case LocalPluginSourceKind.LocalFolder:
                if (exactVersion is not null || gitReference is not null || subdirectory is not null ||
                    !Path.IsPathFullyQualified(location))
                {
                    throw new PluginPackageException(
                        "A local Plugin source must be an absolute directory and cannot use npm, Git or subdirectory options.");
                }
                var sourceRoot = Path.TrimEndingDirectorySeparator(Path.GetFullPath(location));
                if (!Directory.Exists(sourceRoot))
                {
                    throw new PluginPackageException("The local Plugin source directory does not exist.");
                }
                return source with { Location = sourceRoot, ExactVersion = null, GitReference = null, Subdirectory = null };

            case LocalPluginSourceKind.GitRepository:
                if (exactVersion is not null || location.Contains('\\') ||
                    location.Contains('\r') || location.Contains('\n') ||
                    !Uri.TryCreate(location, UriKind.Absolute, out var uri) ||
                    !string.Equals(uri.Scheme, Uri.UriSchemeHttps, StringComparison.OrdinalIgnoreCase) ||
                    string.IsNullOrWhiteSpace(uri.Host) || !string.IsNullOrEmpty(uri.UserInfo) ||
                    !string.IsNullOrEmpty(uri.Query) || !string.IsNullOrEmpty(uri.Fragment))
                {
                    throw new PluginPackageException(
                        "Git Plugin sources must use a credential-free HTTPS repository URL without a query or fragment.");
                }
                if (gitReference is not null &&
                    (gitReference.Length > 256 || gitReference.StartsWith('-') ||
                     gitReference.Contains("..", StringComparison.Ordinal) ||
                     gitReference.Contains("@{", StringComparison.Ordinal) ||
                     !Regex.IsMatch(gitReference, "^[A-Za-z0-9][A-Za-z0-9._/-]*$",
                         RegexOptions.CultureInvariant)))
                {
                    throw new PluginPackageException("The Git reference is invalid.");
                }
                if (subdirectory is not null)
                {
                    if (subdirectory.Contains('\\') ||
                        !string.Equals(NormalizeRelativePath(subdirectory), subdirectory, StringComparison.Ordinal))
                    {
                        throw new PluginPackageException("The Git Plugin subdirectory must be a safe relative path.");
                    }
                }
                return source with
                {
                    Location = location,
                    ExactVersion = null,
                    GitReference = gitReference,
                    Subdirectory = subdirectory,
                };

            case LocalPluginSourceKind.NpmPackage:
                throw new PluginPackageException(
                    "Full npm Agent Plugin sources are not yet supported by this Windows pipeline. Use the pinned npm MCP entry for stdio MCP packages.");
            default:
                throw new PluginPackageException("The Plugin source kind is unsupported.");
        }
    }

    private static string SelectPluginRoot(string acquiredRoot, string? subdirectory)
    {
        var selected = subdirectory is null
            ? acquiredRoot
            : Path.GetFullPath(Path.Combine(acquiredRoot, subdirectory.Replace('/', Path.DirectorySeparatorChar)));
        if (!IsSameOrChild(acquiredRoot, selected) || !Directory.Exists(selected))
        {
            throw new PluginPackageException("The Git Plugin subdirectory does not exist inside the acquired source.");
        }
        RejectReparsePoint(selected);
        return selected;
    }

    private static string? NullIfWhiteSpace(string? value)
    {
        var trimmed = value?.Trim();
        return string.IsNullOrEmpty(trimmed) ? null : trimmed;
    }

    private async Task<LocalPluginInspection> InspectPreparedAsync(
        PreparedLocalPlugin prepared,
        CancellationToken cancellationToken)
    {
        var permissionSnapshot = prepared.Record.DeclaredPermissions
            .ToHashSet(StringComparer.Ordinal);
        permissionSnapshot.Add("process.spawn");
        var components = new List<LocalPluginInspectionComponent>();
        var fingerprintParts = new List<string>
        {
            prepared.Record.ArtifactSha256,
            prepared.SourceSummary,
        };

        for (var index = 0; index < prepared.Manifest.Skills.Count; index++)
        {
            var path = prepared.Manifest.Skills[index].Path
                ?? throw new PluginPackageException("Plugin Skill path is missing.");
            var componentKey = SkillComponentKey(path, index);
            var snapshot = PluginSkillSnapshotLoader.CreateLocalSnapshot(
                prepared.Record, componentKey);
            components.Add(new LocalPluginInspectionComponent(
                componentKey,
                componentKey,
                LocalPluginInspectionComponentKind.Skill,
                Array.Empty<string>()));
            fingerprintParts.Add(componentKey);
            fingerprintParts.Add(CanonicalJson(snapshot));
        }

        foreach (var (componentKey, server) in prepared.Manifest.McpServers
                     .OrderBy(value => value.Key, StringComparer.Ordinal))
        {
            if (!string.Equals(server.EffectiveTransport, "stdio", StringComparison.Ordinal))
            {
                throw new PluginPackageException(
                    "Custom Plugin preflight currently supports only stdio MCP components.");
            }

            var launch = await _manifestLoader.PrepareAsync(
                prepared.Record,
                componentKey,
                componentKey,
                $"custom-plugin-inspection-{Guid.NewGuid():N}",
                workspaceRoot: null,
                permissionSnapshot,
                ownerUserId: "local-user",
                deviceId: "local-device",
                cancellationToken: cancellationToken).ConfigureAwait(false);
            await using var client = _clientFactory.Create(launch);
            try
            {
                await client.StartAsync(cancellationToken).ConfigureAwait(false);
                var initialized = await client.InitializeAsync(cancellationToken).ConfigureAwait(false);
                var tools = initialized.Tools.Select(value => value.Clone()).ToArray();
                var names = tools.Select(ToolName).Order(StringComparer.Ordinal).ToArray();
                if (names.Length == 0 || names.Any(string.IsNullOrWhiteSpace) ||
                    names.Distinct(StringComparer.Ordinal).Count() != names.Length)
                {
                    throw new PluginPackageException("MCP tool names are missing or duplicated.");
                }

                components.Add(new LocalPluginInspectionComponent(
                    componentKey,
                    string.IsNullOrWhiteSpace(server.Agent?.DisplayName)
                        ? componentKey
                        : server.Agent.DisplayName.Trim(),
                    LocalPluginInspectionComponentKind.Mcp,
                    names));
                fingerprintParts.Add(componentKey);
                fingerprintParts.AddRange(tools.Select(CanonicalJson).Order(StringComparer.Ordinal));
            }
            finally
            {
                await client.TerminateAsync().ConfigureAwait(false);
            }
        }

        if (components.Count == 0)
        {
            throw new PluginPackageException("The Plugin does not contain a runnable Skill or stdio MCP component.");
        }

        var fingerprint = Sha256Hex(string.Join('\n', fingerprintParts));
        return new LocalPluginInspection(
            fingerprint,
            string.IsNullOrWhiteSpace(prepared.Manifest.Interface?.DisplayName)
                ? prepared.Manifest.Name.Trim()
                : prepared.Manifest.Interface.DisplayName.Trim(),
            prepared.Manifest.Version.Trim(),
            prepared.SourceSummary,
            components);
    }

    private async Task<InstalledPluginRecord> CommitAsync(
        PreparedLocalPlugin prepared,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        // npm dependency trees routinely contain paths over 120 characters. Two full
        // SHA-256 directory names push otherwise valid packages past path limits in
        // Node's package-scope resolver on Windows. Only new npm-MCP records use 96-bit
        // disk keys; local-folder layouts remain unchanged for upgrade compatibility.
        // Durable records keep the full identities and file hashes in both cases.
        var compactNpmPath = string.Equals(
            prepared.Record.SourceKind, "customMcp", StringComparison.Ordinal);
        var pluginRoot = Path.Combine(
            _rootPath,
            compactNpmPath ? StorageKey(prepared.Record.PluginId) : Sha256Hex(prepared.Record.PluginId));
        var finalPath = Path.Combine(
            pluginRoot,
            compactNpmPath ? StorageKey(prepared.Record.ReleaseId) : Sha256Hex(prepared.Record.ReleaseId));
        var backupPath = Path.Combine(
            _rootPath,
            $".custom-backup-{Path.GetFileName(pluginRoot)}-{Guid.NewGuid():N}");
        var hadPrevious = Directory.Exists(pluginRoot);
        if (hadPrevious)
        {
            Directory.Move(pluginRoot, backupPath);
        }

        try
        {
            Directory.CreateDirectory(pluginRoot);
            Directory.Move(prepared.PackageRoot, finalPath);
            var installed = prepared.Record with
            {
                InstallationPath = finalPath,
                InstalledAt = DateTimeOffset.UtcNow,
            };
            await _store.SaveAsync(installed, cancellationToken).ConfigureAwait(false);
            DeleteDirectory(backupPath);
            return installed;
        }
        catch
        {
            DeleteDirectory(pluginRoot);
            if (hadPrevious && Directory.Exists(backupPath))
            {
                Directory.Move(backupPath, pluginRoot);
            }
            throw;
        }
    }

    private static void ValidateNpmMcpDraft(NpmMcpDraft draft)
    {
        ArgumentNullException.ThrowIfNull(draft);
        if (string.IsNullOrWhiteSpace(draft.DisplayName) || draft.DisplayName.Length > 120 ||
            draft.DisplayName.IndexOf('\0') >= 0 || draft.Description.Length > 2_000 ||
            draft.Description.IndexOf('\0') >= 0 || !SafeIdentifier(draft.ComponentKey) ||
            draft.ComponentKey.Length > 128 || !SafeNpmPackageName(draft.PackageName) ||
            !Regex.IsMatch(draft.PackageVersion,
                "^[0-9]+\\.[0-9]+\\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\\+[0-9A-Za-z.-]+)?$",
                RegexOptions.CultureInvariant) || !SafeIdentifier(draft.BinaryName) ||
            draft.BinaryName.Length > 128 || draft.Arguments.Count > 64 ||
            draft.Arguments.Any(value => value.Length > 2_048 || value.IndexOf('\0') >= 0))
        {
            throw new PluginPackageException("The pinned npm MCP request is invalid.");
        }
    }

    private static bool SafeNpmPackageName(string value)
    {
        if (string.IsNullOrWhiteSpace(value) || value.Length > 214 || value.IndexOf('\0') >= 0)
        {
            return false;
        }
        return Regex.IsMatch(value,
            "^(?:@[a-z0-9][a-z0-9._-]*/)?[a-z0-9][a-z0-9._-]*$",
            RegexOptions.CultureInvariant);
    }

    private static string ResolveNpmBinary(JsonElement package, string binaryName)
    {
        if (!package.TryGetProperty("bin", out var bins))
        {
            throw new PluginPackageException("The pinned npm package does not declare an executable.");
        }
        string? relative = null;
        if (bins.ValueKind == JsonValueKind.String)
        {
            relative = bins.GetString();
        }
        else if (bins.ValueKind == JsonValueKind.Object &&
            bins.TryGetProperty(binaryName, out var selected) &&
            selected.ValueKind == JsonValueKind.String)
        {
            relative = selected.GetString();
        }
        if (string.IsNullOrWhiteSpace(relative))
        {
            throw new PluginPackageException("The pinned npm package does not declare the requested executable.");
        }
        return NormalizeRelativePath(relative);
    }

    private static async Task WriteJsonAsync(
        string path,
        object value,
        CancellationToken cancellationToken)
    {
        await using var stream = new FileStream(
            path, FileMode.Create, FileAccess.Write, FileShare.None, 64 * 1024,
            FileOptions.Asynchronous | FileOptions.SequentialScan);
        await JsonSerializer.SerializeAsync(stream, value, new JsonSerializerOptions
        {
            WriteIndented = true,
        }, cancellationToken).ConfigureAwait(false);
    }

    private static void ValidateManifest(PluginManifest manifest)
    {
        if (manifest.SchemaVersion != 3 ||
            string.IsNullOrWhiteSpace(manifest.Name) || manifest.Name.Length > 200 ||
            string.IsNullOrWhiteSpace(manifest.Version) || manifest.Version.Length > 128 ||
            manifest.McpServers.Count == 0 && manifest.Skills.Count == 0)
        {
            throw new PluginPackageException(
                "The local Plugin must be a schema-v3 package with at least one Skill or MCP component.");
        }
        if (manifest.McpServers.Keys.Any(key =>
                string.IsNullOrWhiteSpace(key) || key.Length > 128 || !SafeIdentifier(key)))
        {
            throw new PluginPackageException("The Plugin contains an invalid MCP component key.");
        }
        if (manifest.McpServers.Values.Any(server =>
                !string.Equals(server.EffectiveTransport, "stdio", StringComparison.Ordinal) ||
                string.IsNullOrWhiteSpace(server.Bin) || server.Environment.Count != 0))
        {
            throw new PluginPackageException(
                "Custom Plugin components must use stdio and may not embed plaintext environment values.");
        }
        var skillKeys = new HashSet<string>(manifest.McpServers.Keys, StringComparer.Ordinal);
        for (var index = 0; index < manifest.Skills.Count; index++)
        {
            var path = manifest.Skills[index].Path;
            if (string.IsNullOrWhiteSpace(path) || !string.Equals(
                    NormalizeRelativePath(path), path.Replace('\\', '/'), StringComparison.Ordinal))
            {
                throw new PluginPackageException("The Plugin contains an invalid Skill path.");
            }
            var componentKey = SkillComponentKey(path, index);
            if (!skillKeys.Add(componentKey))
            {
                throw new PluginPackageException("Plugin Skill and MCP component keys must be unique.");
            }
        }
    }

    private static string SkillComponentKey(string path, int index)
    {
        var name = path.Trim('/').Split('/').LastOrDefault()?.Split('.')[0] ?? "skills";
        var normalized = new string(name.ToLowerInvariant().Select(character =>
            char.IsAsciiLetterOrDigit(character) ? character : '-').ToArray());
        while (normalized.Contains("--", StringComparison.Ordinal))
        {
            normalized = normalized.Replace("--", "-", StringComparison.Ordinal);
        }
        normalized = normalized.Trim('-');
        if (normalized.Length == 0) normalized = "skills";
        return index > 0 && normalized == "skills" ? $"skills-{index + 1}" : normalized;
    }

    private static async Task<IReadOnlyDictionary<string, string>> CopyVerifiedTreeAsync(
        string sourceRoot,
        string destinationRoot,
        CancellationToken cancellationToken)
    {
        var hashes = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        var pending = new Queue<string>();
        pending.Enqueue(sourceRoot);
        var entries = 0;
        long totalBytes = 0;
        while (pending.TryDequeue(out var directory))
        {
            cancellationToken.ThrowIfCancellationRequested();
            RejectReparsePoint(directory);
            foreach (var entry in Directory.EnumerateFileSystemEntries(directory))
            {
                cancellationToken.ThrowIfCancellationRequested();
                if (++entries > MaximumEntries)
                {
                    throw new PluginPackageException("The Plugin source contains too many entries.");
                }
                RejectReparsePoint(entry);
                var relative = NormalizeRelativePath(Path.GetRelativePath(sourceRoot, entry));
                var destination = Path.GetFullPath(Path.Combine(destinationRoot, relative));
                if (!IsSameOrChild(destinationRoot, destination))
                {
                    throw new PluginPackageException("A Plugin source path escapes the isolated package root.");
                }

                if (Directory.Exists(entry))
                {
                    Directory.CreateDirectory(destination);
                    pending.Enqueue(entry);
                    continue;
                }
                if (!File.Exists(entry))
                {
                    throw new PluginPackageException("The Plugin source contains an unsupported filesystem entry.");
                }

                var length = new FileInfo(entry).Length;
                if (length > MaximumFileBytes || (totalBytes = checked(totalBytes + length)) > MaximumPackageBytes)
                {
                    throw new PluginPackageException("The Plugin source exceeds the package size limit.");
                }
                if (hashes.ContainsKey(relative))
                {
                    throw new PluginPackageException("The Plugin source contains a case-colliding path.");
                }
                Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
                await using (var input = new FileStream(
                    entry, FileMode.Open, FileAccess.Read, FileShare.Read, 64 * 1024,
                    FileOptions.Asynchronous | FileOptions.SequentialScan))
                await using (var output = new FileStream(
                    destination, FileMode.CreateNew, FileAccess.Write, FileShare.None, 64 * 1024,
                    FileOptions.Asynchronous | FileOptions.SequentialScan))
                {
                    await input.CopyToAsync(output, 64 * 1024, cancellationToken).ConfigureAwait(false);
                }
                hashes.Add(relative, await HashFileAsync(destination, cancellationToken).ConfigureAwait(false));
            }
        }
        return hashes;
    }

    private static async Task<IReadOnlyDictionary<string, string>> SnapshotVerifiedTreeAsync(
        string root,
        CancellationToken cancellationToken,
        int maximumEntries = MaximumEntries)
    {
        var hashes = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        var pending = new Queue<string>();
        pending.Enqueue(root);
        var entries = 0;
        long totalBytes = 0;
        while (pending.TryDequeue(out var directory))
        {
            cancellationToken.ThrowIfCancellationRequested();
            RejectReparsePoint(directory);
            foreach (var entry in Directory.EnumerateFileSystemEntries(directory))
            {
                cancellationToken.ThrowIfCancellationRequested();
                if (++entries > maximumEntries)
                {
                    throw new PluginPackageException("The normalized Plugin contains too many entries.");
                }
                RejectReparsePoint(entry);
                var relative = NormalizeRelativePath(Path.GetRelativePath(root, entry));
                if (Directory.Exists(entry))
                {
                    pending.Enqueue(entry);
                    continue;
                }
                if (!File.Exists(entry))
                {
                    throw new PluginPackageException("The normalized Plugin contains an unsupported filesystem entry.");
                }
                var length = new FileInfo(entry).Length;
                if (length > MaximumFileBytes || (totalBytes = checked(totalBytes + length)) > MaximumPackageBytes)
                {
                    throw new PluginPackageException("The normalized Plugin exceeds the package size limit.");
                }
                if (!hashes.TryAdd(relative, await HashFileAsync(entry, cancellationToken).ConfigureAwait(false)))
                {
                    throw new PluginPackageException("The normalized Plugin contains a case-colliding path.");
                }
            }
        }
        return hashes;
    }

    private static string ToolName(JsonElement tool) =>
        tool.ValueKind == JsonValueKind.Object &&
        tool.TryGetProperty("name", out var name) &&
        name.ValueKind == JsonValueKind.String
            ? name.GetString()?.Trim() ?? string.Empty
            : string.Empty;

    private static string CanonicalJson(JsonElement value)
    {
        using var buffer = new MemoryStream();
        using (var writer = new Utf8JsonWriter(buffer))
        {
            WriteCanonical(writer, value);
        }
        return Encoding.UTF8.GetString(buffer.ToArray());
    }

    private static void WriteCanonical(Utf8JsonWriter writer, JsonElement value)
    {
        switch (value.ValueKind)
        {
            case JsonValueKind.Object:
                writer.WriteStartObject();
                foreach (var property in value.EnumerateObject().OrderBy(item => item.Name, StringComparer.Ordinal))
                {
                    writer.WritePropertyName(property.Name);
                    WriteCanonical(writer, property.Value);
                }
                writer.WriteEndObject();
                break;
            case JsonValueKind.Array:
                writer.WriteStartArray();
                foreach (var item in value.EnumerateArray()) WriteCanonical(writer, item);
                writer.WriteEndArray();
                break;
            default:
                value.WriteTo(writer);
                break;
        }
    }

    private static string NormalizeRelativePath(string value)
    {
        var normalized = value.Replace('\\', '/');
        if (normalized.Length == 0 || normalized.Length > 1_024 ||
            normalized.StartsWith('/') || normalized.Contains(':') ||
            normalized.Split('/').Any(segment =>
                segment.Length == 0 || segment is "." or ".." ||
                segment.EndsWith(' ') || segment.EndsWith('.')))
        {
            throw new PluginPackageException("The Plugin source contains an unsafe Windows path.");
        }
        return normalized;
    }

    private static void RejectReparsePoint(string path)
    {
        if ((File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0)
        {
            throw new PluginPackageException("The Plugin source contains a reparse point.");
        }
    }

    private static bool IsSameOrChild(string parent, string candidate)
    {
        var normalizedParent = Path.TrimEndingDirectorySeparator(Path.GetFullPath(parent));
        var normalizedCandidate = Path.TrimEndingDirectorySeparator(Path.GetFullPath(candidate));
        return normalizedCandidate.Equals(normalizedParent, StringComparison.OrdinalIgnoreCase) ||
            normalizedCandidate.StartsWith(
                normalizedParent + Path.DirectorySeparatorChar,
                StringComparison.OrdinalIgnoreCase);
    }

    private static string HashCanonicalFiles(IReadOnlyDictionary<string, string> files) =>
        Sha256Hex(string.Join('\n', files.OrderBy(value => value.Key, StringComparer.OrdinalIgnoreCase)
            .Select(value => $"{value.Key.Replace('\\', '/')}\0{value.Value}")));

    private static async Task<string> HashFileAsync(string path, CancellationToken cancellationToken)
    {
        await using var stream = new FileStream(
            path, FileMode.Open, FileAccess.Read, FileShare.Read, 64 * 1024,
            FileOptions.Asynchronous | FileOptions.SequentialScan);
        return Convert.ToHexString(await SHA256.HashDataAsync(stream, cancellationToken).ConfigureAwait(false))
            .ToLowerInvariant();
    }

    private static string Sha256Hex(string value) =>
        Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(value))).ToLowerInvariant();

    private static string StorageKey(string value) => Sha256Hex(value)[..24];

    private static bool IsSha256(string value) =>
        value is { Length: 64 } && value.All(Uri.IsHexDigit);

    private static bool SafeIdentifier(string value) =>
        char.IsAsciiLetterOrDigit(value[0]) &&
        value.All(character => char.IsAsciiLetterOrDigit(character) || character is '.' or '_' or '-');

    private static void DeleteDirectory(string path)
    {
        if (string.IsNullOrWhiteSpace(path) || !Directory.Exists(path)) return;
        try
        {
            DeleteDirectoryTree(path);
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }

    private static void DeleteDirectoryTree(string path)
    {
        foreach (var entry in Directory.EnumerateFileSystemEntries(path))
        {
            var attributes = File.GetAttributes(entry);
            if ((attributes & FileAttributes.Directory) != 0 &&
                (attributes & FileAttributes.ReparsePoint) == 0)
            {
                DeleteDirectoryTree(entry);
                continue;
            }

            File.SetAttributes(entry, attributes & ~FileAttributes.ReadOnly);
            if ((attributes & FileAttributes.Directory) != 0)
            {
                Directory.Delete(entry, recursive: false);
            }
            else
            {
                File.Delete(entry);
            }
        }
        File.SetAttributes(path, File.GetAttributes(path) & ~FileAttributes.ReadOnly);
        Directory.Delete(path, recursive: false);
    }
}

internal sealed class WindowsNpmMcpPackageAcquirer : INpmMcpPackageAcquirer
{
    private static readonly TimeSpan InstallTimeout = TimeSpan.FromMinutes(5);
    private const int MaximumDiagnosticCharacters = 4_000;

    public async Task AcquireAsync(
        NpmMcpDraft draft,
        string packageRoot,
        CancellationToken cancellationToken)
    {
        if (!OperatingSystem.IsWindows())
        {
            throw new PluginPackageException("Pinned npm MCP acquisition requires Windows.");
        }
        var node = ResolveFromPath("node.exe")
            ?? throw new PluginPackageException("Node.js is required to acquire a pinned npm MCP.");
        var npmCli = ResolveNpmCli()
            ?? throw new PluginPackageException("npm is required to acquire a pinned npm MCP.");
        var start = new ProcessStartInfo
        {
            FileName = node,
            WorkingDirectory = packageRoot,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        start.ArgumentList.Add(npmCli);
        foreach (var argument in new[]
        {
            "install", "--ignore-scripts", "--omit=dev", "--no-audit", "--no-fund",
            "--save-exact", "--package-lock=true",
        })
        {
            start.ArgumentList.Add(argument);
        }
        start.Environment["NPM_CONFIG_IGNORE_SCRIPTS"] = "true";
        start.Environment["NPM_CONFIG_AUDIT"] = "false";
        start.Environment["NPM_CONFIG_FUND"] = "false";

        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(InstallTimeout);
        using var process = Process.Start(start)
            ?? throw new PluginPackageException("npm acquisition could not be started.");
        using var job = new WindowsProcessJob();
        try
        {
            job.Assign(process);
            process.StandardInput.Close();
            var standardOutput = process.StandardOutput.ReadToEndAsync(timeout.Token);
            var standardError = process.StandardError.ReadToEndAsync(timeout.Token);
            await process.WaitForExitAsync(timeout.Token).ConfigureAwait(false);
            var output = await standardOutput.ConfigureAwait(false);
            var error = await standardError.ConfigureAwait(false);
            if (process.ExitCode != 0)
            {
                throw new PluginPackageException(
                    "npm could not acquire the pinned package: " + Diagnostic(error, output));
            }
        }
        catch (OperationCanceledException exception) when (timeout.IsCancellationRequested)
        {
            TryTerminate(job);
            if (cancellationToken.IsCancellationRequested) throw;
            throw new PluginPackageException("npm acquisition timed out.", exception);
        }
        catch
        {
            TryTerminate(job);
            throw;
        }
    }

    private static string Diagnostic(string error, string output)
    {
        var value = string.IsNullOrWhiteSpace(error) ? output : error;
        value = value.Replace('\r', ' ').Replace('\n', ' ').Trim();
        if (value.Length == 0) return "the process returned a non-zero exit code.";
        return value.Length <= MaximumDiagnosticCharacters
            ? value
            : value[^MaximumDiagnosticCharacters..];
    }

    private static string? ResolveNpmCli()
    {
        var npmCommand = ResolveFromPath("npm.cmd");
        if (npmCommand is null) return null;
        var root = Path.GetDirectoryName(npmCommand);
        if (root is null) return null;
        var candidate = Path.Combine(root, "node_modules", "npm", "bin", "npm-cli.js");
        return File.Exists(candidate) ? candidate : null;
    }

    private static string? ResolveFromPath(string fileName)
    {
        foreach (var entry in (Environment.GetEnvironmentVariable("PATH") ?? string.Empty)
                     .Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
        {
            try
            {
                var candidate = Path.Combine(entry.Trim('"'), fileName);
                if (File.Exists(candidate)) return Path.GetFullPath(candidate);
            }
            catch (Exception exception) when (
                exception is ArgumentException or NotSupportedException or PathTooLongException)
            {
            }
        }
        return null;
    }

    private static void TryTerminate(WindowsProcessJob job)
    {
        try
        {
            job.Terminate();
        }
        catch (Exception exception) when (exception is InvalidOperationException or System.ComponentModel.Win32Exception)
        {
        }
    }
}

internal sealed class WindowsGitPluginSourceAcquirer : IGitPluginSourceAcquirer
{
    private static readonly TimeSpan FetchTimeout = TimeSpan.FromMinutes(5);

    public async Task AcquireAsync(
        string repositoryUrl,
        string reference,
        string destinationRoot,
        CancellationToken cancellationToken)
    {
        if (!OperatingSystem.IsWindows())
        {
            throw new PluginPackageException("Git Plugin acquisition requires Windows.");
        }
        var git = ResolveGit()
            ?? throw new PluginPackageException("Git for Windows is required to acquire a Git Plugin source.");
        if (!Directory.Exists(destinationRoot) || Directory.EnumerateFileSystemEntries(destinationRoot).Any())
        {
            throw new PluginPackageException("Git acquisition requires an empty isolated staging directory.");
        }

        await RunAsync(git, destinationRoot, ["init", "--quiet"], cancellationToken)
            .ConfigureAwait(false);
        await RunAsync(git, destinationRoot,
            ["remote", "add", "origin", repositoryUrl], cancellationToken).ConfigureAwait(false);
        await RunAsync(git, destinationRoot,
            [
                "-c", "core.hooksPath=NUL",
                "-c", "protocol.allow=never",
                "-c", "protocol.https.allow=always",
                "-c", "protocol.file.allow=never",
                "-c", "http.version=HTTP/1.1",
                "fetch", "--quiet", "--depth", "1", "--no-tags", "--no-recurse-submodules",
                "origin", reference,
            ], cancellationToken).ConfigureAwait(false);
        await RunAsync(git, destinationRoot,
            [
                "-c", "core.hooksPath=NUL",
                "-c", "protocol.allow=never",
                "-c", "protocol.https.allow=always",
                "-c", "protocol.file.allow=never",
                "checkout", "--quiet", "--detach", "FETCH_HEAD",
            ], cancellationToken).ConfigureAwait(false);
    }

    private static async Task RunAsync(
        string git,
        string workingDirectory,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        var start = new ProcessStartInfo
        {
            FileName = git,
            WorkingDirectory = workingDirectory,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        foreach (var argument in arguments) start.ArgumentList.Add(argument);
        start.Environment["GIT_CONFIG_GLOBAL"] = "NUL";
        start.Environment["GIT_CONFIG_SYSTEM"] = "NUL";
        start.Environment["GIT_TERMINAL_PROMPT"] = "0";
        start.Environment["GCM_INTERACTIVE"] = "Never";
        start.Environment["GIT_PROTOCOL_FROM_USER"] = "0";

        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(FetchTimeout);
        using var process = Process.Start(start)
            ?? throw new PluginPackageException("Git acquisition could not be started.");
        using var job = new WindowsProcessJob();
        try
        {
            job.Assign(process);
            process.StandardInput.Close();
            var standardOutput = process.StandardOutput.ReadToEndAsync(timeout.Token);
            var standardError = process.StandardError.ReadToEndAsync(timeout.Token);
            await process.WaitForExitAsync(timeout.Token).ConfigureAwait(false);
            _ = await standardOutput.ConfigureAwait(false);
            _ = await standardError.ConfigureAwait(false);
            if (process.ExitCode != 0)
            {
                throw new PluginPackageException(
                    $"Git could not acquire the pinned source (exit code {process.ExitCode}).");
            }
        }
        catch (OperationCanceledException exception) when (timeout.IsCancellationRequested)
        {
            TryTerminate(job);
            await WaitForExitAfterTerminationAsync(process).ConfigureAwait(false);
            if (cancellationToken.IsCancellationRequested) throw;
            throw new PluginPackageException("Git acquisition timed out.", exception);
        }
        catch
        {
            TryTerminate(job);
            await WaitForExitAfterTerminationAsync(process).ConfigureAwait(false);
            throw;
        }
    }

    private static string? ResolveGit()
    {
        foreach (var entry in (Environment.GetEnvironmentVariable("PATH") ?? string.Empty)
                     .Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
        {
            try
            {
                var candidate = Path.Combine(entry.Trim('"'), "git.exe");
                if (File.Exists(candidate)) return Path.GetFullPath(candidate);
            }
            catch (Exception exception) when (
                exception is ArgumentException or NotSupportedException or PathTooLongException)
            {
            }
        }
        return null;
    }

    private static void TryTerminate(WindowsProcessJob job)
    {
        try
        {
            job.Terminate();
        }
        catch (Exception exception) when (
            exception is InvalidOperationException or System.ComponentModel.Win32Exception)
        {
        }
    }

    private static async Task WaitForExitAfterTerminationAsync(Process process)
    {
        if (process.HasExited) return;
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        try
        {
            await process.WaitForExitAsync(timeout.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (timeout.IsCancellationRequested)
        {
        }
    }
}
