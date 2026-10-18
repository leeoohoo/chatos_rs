using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace ChatOS.Connector.Plugins;

/// <summary>
/// Normalizes portable Agent Plugin, Codex/Claude Plugin, standalone Skill and
/// mainstream local MCP configuration layouts into the immutable ChatOS schema-v3
/// package used by the Windows runtime. This only operates on the isolated staging
/// copy created by <see cref="WindowsCustomPluginInstaller"/>.
/// </summary>
internal static class WindowsAgentPluginCompatibilityImporter
{
    private const int MaximumJsonBytes = 2 * 1024 * 1024;
    private const int MaximumSkillBytes = 256 * 1024;
    private static readonly JsonSerializerOptions WriteOptions = new(JsonSerializerDefaults.Web)
    {
        WriteIndented = true,
    };

    internal sealed record Result(PluginManifest Manifest, string FormatName);

    private sealed record Identity(
        string Name,
        string Version,
        string Description,
        string DisplayName,
        string FormatName,
        JsonElement? RawManifest);

    private sealed record NormalizedMcpServer(
        string ComponentKey,
        string DisplayName,
        string Description,
        string LauncherName,
        IReadOnlyList<string> Arguments,
        IReadOnlyDictionary<string, string> Environment);

    private static readonly (string Path, string FormatName)[] IdentityCandidates =
    [
        ("plugin.json", "Agent Plugin"),
        (".codex-plugin/plugin.json", "Codex Plugin"),
        (".claude-plugin/plugin.json", "Claude Plugin"),
        ("package.json", "npm Agent Plugin"),
    ];

    private static readonly string[] McpConfigurationCandidates =
    [
        "mcp.json",
        ".mcp.json",
        ".cursor/mcp.json",
        ".vscode/mcp.json",
        ".gemini/settings.json",
        "claude_desktop_config.json",
        "mcp_config.json",
        "cline_mcp_settings.json",
        "opencode.json",
    ];

    internal static Result NormalizePackage(string packageRoot)
    {
        var nativeManifestPath = Path.Combine(packageRoot, "chatos.plugin.json");
        if (File.Exists(nativeManifestPath))
        {
            return new Result(ReadManifest(nativeManifestPath), "ChatOS Plugin");
        }

        var identity = DiscoverIdentity(packageRoot);
        var skills = DiscoverSkills(packageRoot, identity.Name);
        var mcpServers = DiscoverMcpServers(packageRoot, identity.RawManifest);
        if (skills.Count == 0 && mcpServers.Count == 0)
        {
            throw new PluginPackageException(
                "No importable Skill or MCP was found. Add SKILL.md, skills/*/SKILL.md, or a supported MCP configuration.");
        }

        WriteLaunchMetadata(packageRoot, mcpServers);
        var manifest = new PluginManifest
        {
            SchemaVersion = 3,
            Name = identity.Name,
            Version = identity.Version,
            Description = identity.Description,
            Skills = skills.Select(path => new PluginPathReference { Path = path }).ToArray(),
            McpServers = mcpServers.ToDictionary(
                value => value.ComponentKey,
                value => new PluginMcpServer
                {
                    Type = "stdio",
                    Bin = value.LauncherName,
                    Arguments = value.Arguments,
                    Environment = value.Environment,
                    Agent = new PluginAgentPresentation
                    {
                        DisplayName = value.DisplayName,
                        Description = value.Description,
                    },
                },
                StringComparer.Ordinal),
            Interface = new PluginInterface { DisplayName = identity.DisplayName },
            Permissions = mcpServers.Count == 0
                ? Array.Empty<PluginPermission>()
                :
                [
                    new PluginPermission
                    {
                        Permission = "process.spawn",
                        Required = true,
                        Reason = "Start the local MCP server explicitly imported by the user.",
                        Components = mcpServers.Select(value => value.ComponentKey)
                            .Order(StringComparer.Ordinal).ToArray(),
                    },
                ],
        };
        File.WriteAllBytes(
            nativeManifestPath,
            JsonSerializer.SerializeToUtf8Bytes(manifest, WriteOptions));
        return new Result(ReadManifest(nativeManifestPath), identity.FormatName);
    }

    private static Identity DiscoverIdentity(string root)
    {
        foreach (var candidate in IdentityCandidates)
        {
            var path = PackagePath(root, candidate.Path);
            if (!File.Exists(path)) continue;
            var manifest = ReadObject(path);
            var rawName = RequiredString(manifest, "name", $"{candidate.FormatName} manifest");
            var rawVersion = RequiredString(manifest, "version", $"{candidate.FormatName} manifest");
            var name = NormalizeIdentifier(rawName, "Plugin name");
            var version = rawVersion.Trim();
            if (version.Length == 0 || version.Length > 128)
            {
                throw new PluginPackageException("Plugin version is invalid.");
            }
            var description = OptionalString(manifest, "description")?.Trim() ?? string.Empty;
            return new Identity(
                name,
                version,
                description.Length <= 2_000 ? description : description[..2_000],
                DisplayName(manifest) ?? rawName,
                candidate.FormatName,
                manifest.Clone());
        }

        var hasMcp = McpConfigurationCandidates.Any(value => File.Exists(PackagePath(root, value)));
        var hasRootSkill = File.Exists(Path.Combine(root, "SKILL.md"));
        if (!hasMcp && !hasRootSkill)
        {
            throw new PluginPackageException(
                "The folder is not a supported Plugin, Skill, or MCP configuration.");
        }
        var folderName = Path.GetFileName(Path.TrimEndingDirectorySeparator(root));
        if (string.IsNullOrWhiteSpace(folderName)) folderName = "local-mcp";
        return new Identity(
            NormalizeIdentifier(folderName, "Folder name"),
            "0.0.0-local",
            "Imported standalone Skill or MCP configuration",
            folderName,
            hasRootSkill && !hasMcp ? "Standalone Skill" : "Standalone MCP",
            null);
    }

    private static IReadOnlyList<string> DiscoverSkills(string root, string identityName)
    {
        var rootSkill = Path.Combine(root, "SKILL.md");
        if (File.Exists(rootSkill))
        {
            var collectionName = StandaloneSkillName(rootSkill, identityName);
            var collection = Path.Combine(root, "skills", collectionName);
            if (Directory.Exists(collection) || File.Exists(collection))
            {
                throw new PluginPackageException(
                    $"Root SKILL.md conflicts with skills/{collectionName}.");
            }
            Directory.CreateDirectory(collection);
            File.Copy(rootSkill, Path.Combine(collection, "SKILL.md"));
            foreach (var resourceName in new[] { "scripts", "references", "assets" })
            {
                var source = Path.Combine(root, resourceName);
                if (!Directory.Exists(source)) continue;
                CopyDirectory(source, Path.Combine(collection, resourceName));
            }
        }

        var skillsRoot = Path.Combine(root, "skills");
        if (!Directory.Exists(skillsRoot)) return Array.Empty<string>();
        var result = new List<string>();
        foreach (var directory in Directory.EnumerateDirectories(skillsRoot).Order(StringComparer.Ordinal))
        {
            RejectReparsePoint(directory);
            var skillFile = Path.Combine(directory, "SKILL.md");
            if (!File.Exists(skillFile)) continue;
            _ = StandaloneSkillName(skillFile, Path.GetFileName(directory));
            var name = NormalizeIdentifier(Path.GetFileName(directory), "Skill folder name");
            if (!string.Equals(name, Path.GetFileName(directory), StringComparison.Ordinal))
            {
                throw new PluginPackageException("Skill folder names must already be safe identifiers.");
            }
            result.Add($"skills/{name}");
        }
        return result;
    }

    private static IReadOnlyList<NormalizedMcpServer> DiscoverMcpServers(
        string root,
        JsonElement? inlineManifest)
    {
        JsonElement? rawServers = null;
        foreach (var candidate in McpConfigurationCandidates)
        {
            var path = PackagePath(root, candidate);
            if (!File.Exists(path)) continue;
            var document = ReadObject(path);
            rawServers = FirstObject(document, "mcpServers", "servers", "mcp");
            if (rawServers is null)
            {
                throw new PluginPackageException(
                    $"{candidate} does not contain an mcpServers, servers, or mcp object.");
            }
            break;
        }
        if (rawServers is null && inlineManifest is { } inline)
        {
            rawServers = FirstObject(inline, "mcpServers");
        }
        if (rawServers is null) return Array.Empty<NormalizedMcpServer>();

        var properties = rawServers.Value.EnumerateObject().OrderBy(value => value.Name, StringComparer.Ordinal).ToArray();
        if (properties.Length > 64)
        {
            throw new PluginPackageException("The MCP configuration contains more than 64 servers.");
        }
        var componentKeys = new HashSet<string>(StringComparer.Ordinal);
        var result = new List<NormalizedMcpServer>();
        foreach (var property in properties)
        {
            if (property.Value.ValueKind != JsonValueKind.Object)
            {
                throw new PluginPackageException($"MCP {property.Name} must be a JSON object.");
            }
            var declaredType = (OptionalString(property.Value, "type") ?? "stdio").Trim().ToLowerInvariant();
            var transport = declaredType == "local" ? "stdio" : declaredType;
            if (transport != "stdio")
            {
                throw new PluginPackageException(
                    $"MCP {property.Name} uses {transport}; this importer accepts only local stdio MCP servers.");
            }

            var command = CommandAndInitialArguments(property.Name, property.Value);
            if (command.Count == 0)
            {
                throw new PluginPackageException($"MCP {property.Name} does not declare a command.");
            }
            var componentKey = NormalizeIdentifier(property.Name, "MCP server name");
            if (!componentKeys.Add(componentKey))
            {
                throw new PluginPackageException(
                    $"MCP server names collide after normalization: {property.Name}.");
            }
            ValidateCommand(command[0], root, property.Name);
            var arguments = command.Skip(1)
                .Concat(StringArray(property.Value, "args", $"{property.Name}.args"))
                .ToArray();
            var environment = StringDictionary(
                property.Value,
                property.Value.TryGetProperty("env", out _) ? "env" : "environment",
                $"{property.Name}.env");
            result.Add(new NormalizedMcpServer(
                componentKey,
                property.Name,
                OptionalString(property.Value, "description") ?? "Imported MCP server",
                $"compat-{componentKey}",
                arguments,
                environment));
            WriteLauncher(root, componentKey, command[0]);
        }
        return result;
    }

    private static IReadOnlyList<string> CommandAndInitialArguments(string serverName, JsonElement server)
    {
        if (!server.TryGetProperty("command", out var command)) return Array.Empty<string>();
        if (command.ValueKind == JsonValueKind.String)
        {
            return [BoundedString(command.GetString(), $"{serverName}.command")];
        }
        if (command.ValueKind != JsonValueKind.Array)
        {
            throw new PluginPackageException($"{serverName}.command must be a string or string array.");
        }
        var values = JsonStringArray(command, $"{serverName}.command");
        if (values.Count == 0)
        {
            throw new PluginPackageException($"{serverName}.command cannot be empty.");
        }
        return values;
    }

    private static void ValidateCommand(string command, string root, string serverName)
    {
        var trimmed = command.Trim();
        if (trimmed.Length == 0 || trimmed.Length > 2_048 || trimmed.Contains('\0'))
        {
            throw new PluginPackageException($"MCP {serverName} command is invalid.");
        }
        if (trimmed.All(character => char.IsAsciiLetterOrDigit(character) || character is '.' or '_' or '+' or '-'))
        {
            return;
        }
        var relative = NormalizePluginPath(trimmed);
        var path = PackagePath(root, relative);
        if (!File.Exists(path) || (File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0)
        {
            throw new PluginPackageException(
                $"MCP {serverName} command must be a PATH command or a file inside the Plugin package.");
        }
    }

    private static void WriteLauncher(string root, string componentKey, string command)
    {
        var bin = Path.Combine(root, "bin");
        Directory.CreateDirectory(bin);
        var launcherPath = Path.Combine(bin, $"compat-{componentKey}.mjs");
        var encodedCommand = JsonSerializer.Serialize(command.Trim());
        var source = $$"""
        import path from "node:path";
        import process from "node:process";
        import { spawn } from "node:child_process";

        const packageRoot = process.env.CHATOS_PLUGIN_ROOT;
        if (!packageRoot) throw new Error("CHATOS_PLUGIN_ROOT is unavailable");
        const expand = (value) => {
          for (const prefix of ["${PLUGIN_ROOT}/", "$PLUGIN_ROOT/", "${CLAUDE_PLUGIN_ROOT}/", "$CLAUDE_PLUGIN_ROOT/"]) {
            if (value.startsWith(prefix)) return path.join(packageRoot, value.slice(prefix.length));
          }
          if (value.startsWith("./") || value.startsWith(".\\")) return path.join(packageRoot, value.slice(2));
          return value;
        };
        const child = spawn(expand({{encodedCommand}}), process.argv.slice(2).map(expand), {
          stdio: "inherit",
          shell: false,
          windowsHide: true,
          env: process.env,
        });
        child.once("error", (error) => { console.error(error.message); process.exitCode = 1; });
        child.once("exit", (code, signal) => {
          if (signal) process.kill(process.pid, signal);
          else process.exitCode = code ?? 1;
        });
        """;
        File.WriteAllText(launcherPath, source, new UTF8Encoding(false));
    }

    private static void WriteLaunchMetadata(string root, IReadOnlyList<NormalizedMcpServer> servers)
    {
        if (servers.Count == 0) return;
        var packagePath = Path.Combine(root, "package.json");
        JsonObject package;
        if (File.Exists(packagePath))
        {
            var bytes = ReadBoundedBytes(packagePath, MaximumJsonBytes);
            package = JsonNode.Parse(bytes)?.AsObject()
                ?? throw new PluginPackageException("package.json must be a JSON object.");
        }
        else
        {
            package = new JsonObject
            {
                ["name"] = "chatos-imported-agent-plugin",
                ["version"] = "0.0.0-local",
                ["private"] = true,
            };
        }
        JsonObject bins;
        if (package["bin"] is JsonObject existingBins)
        {
            bins = existingBins;
        }
        else if (package["bin"] is JsonValue existingBin &&
                 existingBin.TryGetValue<string>(out var existingBinPath) &&
                 !string.IsNullOrWhiteSpace(existingBinPath))
        {
            var packageName = package["name"] is JsonValue packageNameValue &&
                packageNameValue.TryGetValue<string>(out var parsedPackageName)
                    ? parsedPackageName
                    : "plugin";
            var binName = packageName[(packageName.LastIndexOf('/') + 1)..];
            bins = new JsonObject { [binName] = existingBinPath };
        }
        else if (package["bin"] is null)
        {
            bins = new JsonObject();
        }
        else
        {
            throw new PluginPackageException("package.json bin must be a string or object.");
        }
        foreach (var server in servers)
        {
            if (bins.ContainsKey(server.LauncherName))
            {
                throw new PluginPackageException(
                    $"package.json already declares the generated bin {server.LauncherName}.");
            }
            bins[server.LauncherName] = $"bin/{server.LauncherName}.mjs";
        }
        package["bin"] = bins;
        File.WriteAllText(packagePath, package.ToJsonString(WriteOptions), new UTF8Encoding(false));
    }

    private static string StandaloneSkillName(string path, string fallback)
    {
        var bytes = ReadBoundedBytes(path, MaximumSkillBytes);
        string text;
        try
        {
            text = new UTF8Encoding(false, true).GetString(bytes);
        }
        catch (DecoderFallbackException exception)
        {
            throw new PluginPackageException("SKILL.md must be UTF-8 text.", exception);
        }
        var normalized = text.Replace("\r\n", "\n", StringComparison.Ordinal);
        var frontmatterEnd = normalized.IndexOf("\n---\n", StringComparison.Ordinal);
        var frontmatter = frontmatterEnd >= 0 ? normalized[..frontmatterEnd] : normalized;
        var declaredName = frontmatter.Split('\n').Select(line => line.Trim())
            .FirstOrDefault(line => line.StartsWith("name:", StringComparison.Ordinal))?
            ["name:".Length..].Trim().Trim('"', '\'');
        return NormalizeIdentifier(string.IsNullOrWhiteSpace(declaredName) ? fallback : declaredName, "Skill name");
    }

    private static void CopyDirectory(string source, string destination)
    {
        RejectReparsePoint(source);
        Directory.CreateDirectory(destination);
        foreach (var directory in Directory.EnumerateDirectories(source))
        {
            RejectReparsePoint(directory);
            CopyDirectory(directory, Path.Combine(destination, Path.GetFileName(directory)));
        }
        foreach (var file in Directory.EnumerateFiles(source))
        {
            RejectReparsePoint(file);
            File.Copy(file, Path.Combine(destination, Path.GetFileName(file)));
        }
    }

    private static PluginManifest ReadManifest(string path)
    {
        try
        {
            return JsonSerializer.Deserialize<PluginManifest>(
                ReadBoundedBytes(path, MaximumJsonBytes),
                new JsonSerializerOptions(JsonSerializerDefaults.Web) { PropertyNameCaseInsensitive = true })
                ?? throw new JsonException("Plugin manifest is empty.");
        }
        catch (JsonException exception)
        {
            throw new PluginPackageException("Plugin manifest is invalid.", exception);
        }
    }

    private static JsonElement ReadObject(string path)
    {
        try
        {
            using var document = JsonDocument.Parse(ReadBoundedBytes(path, MaximumJsonBytes));
            if (document.RootElement.ValueKind != JsonValueKind.Object)
            {
                throw new JsonException("Root must be an object.");
            }
            return document.RootElement.Clone();
        }
        catch (JsonException exception)
        {
            throw new PluginPackageException($"{Path.GetFileName(path)} must be a JSON object.", exception);
        }
    }

    private static byte[] ReadBoundedBytes(string path, int maximumBytes)
    {
        var info = new FileInfo(path);
        if (!info.Exists || info.Length <= 0 || info.Length > maximumBytes ||
            (info.Attributes & FileAttributes.ReparsePoint) != 0)
        {
            throw new PluginPackageException($"{Path.GetFileName(path)} is missing, unsafe, or too large.");
        }
        return File.ReadAllBytes(path);
    }

    private static JsonElement? FirstObject(JsonElement parent, params string[] names)
    {
        foreach (var name in names)
        {
            if (parent.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.Object)
            {
                return value.Clone();
            }
        }
        return null;
    }

    private static string RequiredString(JsonElement value, string property, string source)
    {
        var result = OptionalString(value, property);
        if (string.IsNullOrWhiteSpace(result))
        {
            throw new PluginPackageException($"{source} must declare {property}.");
        }
        return result;
    }

    private static string? OptionalString(JsonElement value, string property) =>
        value.TryGetProperty(property, out var result) && result.ValueKind == JsonValueKind.String
            ? result.GetString()
            : null;

    private static string? DisplayName(JsonElement manifest)
    {
        if (manifest.TryGetProperty("extensions", out var extensions) &&
            extensions.ValueKind == JsonValueKind.Object &&
            extensions.TryGetProperty("com.openai", out var openAi) &&
            openAi.ValueKind == JsonValueKind.Object &&
            openAi.TryGetProperty("interface", out var openAiInterface) &&
            openAiInterface.ValueKind == JsonValueKind.Object &&
            !string.IsNullOrWhiteSpace(OptionalString(openAiInterface, "displayName")))
        {
            return OptionalString(openAiInterface, "displayName")!.Trim();
        }
        if (manifest.TryGetProperty("interface", out var pluginInterface) &&
            pluginInterface.ValueKind == JsonValueKind.Object &&
            !string.IsNullOrWhiteSpace(OptionalString(pluginInterface, "displayName")))
        {
            return OptionalString(pluginInterface, "displayName")!.Trim();
        }
        return OptionalString(manifest, "displayName")?.Trim();
    }

    private static IReadOnlyList<string> StringArray(JsonElement parent, string property, string field) =>
        parent.TryGetProperty(property, out var value) ? JsonStringArray(value, field) : Array.Empty<string>();

    private static IReadOnlyList<string> JsonStringArray(JsonElement value, string field)
    {
        if (value.ValueKind != JsonValueKind.Array || value.GetArrayLength() > 64)
        {
            throw new PluginPackageException($"{field} must be an array of at most 64 strings.");
        }
        return value.EnumerateArray().Select(item =>
        {
            if (item.ValueKind != JsonValueKind.String)
            {
                throw new PluginPackageException($"{field} must contain only strings.");
            }
            return BoundedString(item.GetString(), field);
        }).ToArray();
    }

    private static IReadOnlyDictionary<string, string> StringDictionary(
        JsonElement parent,
        string property,
        string field)
    {
        if (!parent.TryGetProperty(property, out var value)) return new Dictionary<string, string>();
        if (value.ValueKind != JsonValueKind.Object || value.EnumerateObject().Count() > 64)
        {
            throw new PluginPackageException($"{field} must be an object with at most 64 string values.");
        }
        var result = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (var item in value.EnumerateObject())
        {
            if (!SafeEnvironmentName(item.Name) || item.Value.ValueKind != JsonValueKind.String)
            {
                throw new PluginPackageException($"{field} contains an invalid environment entry.");
            }
            var text = item.Value.GetString() ?? string.Empty;
            if (text.Length > 8_192 || text.Contains('\0'))
            {
                throw new PluginPackageException($"{field} contains an invalid environment value.");
            }
            result.Add(item.Name, text);
        }
        return result;
    }

    private static string BoundedString(string? value, string field)
    {
        var result = value ?? string.Empty;
        if (result.Length > 2_048 || result.Contains('\0'))
        {
            throw new PluginPackageException($"{field} contains an invalid string.");
        }
        return result;
    }

    private static string NormalizeIdentifier(string raw, string field)
    {
        var mapped = raw.Trim().ToLowerInvariant().Select(character =>
            char.IsAsciiLetterOrDigit(character) || character is '.' or '_' or '-' ? character : '-').ToArray();
        var value = new string(mapped).Trim('-', '.', '_');
        if (value.Length == 0 || value.Length > 128 || !char.IsAsciiLetterOrDigit(value[0]) ||
            value.Any(character => !(char.IsAsciiLetterOrDigit(character) || character is '.' or '_' or '-')))
        {
            throw new PluginPackageException($"{field} cannot be converted to a safe identifier.");
        }
        return value;
    }

    private static string NormalizePluginPath(string value)
    {
        var normalized = value.Replace('\\', '/');
        foreach (var prefix in new[]
                 {
                     "${PLUGIN_ROOT}/", "$PLUGIN_ROOT/",
                     "${CLAUDE_PLUGIN_ROOT}/", "$CLAUDE_PLUGIN_ROOT/",
                 })
        {
            if (normalized.StartsWith(prefix, StringComparison.Ordinal))
            {
                normalized = normalized[prefix.Length..];
                break;
            }
        }
        while (normalized.StartsWith("./", StringComparison.Ordinal)) normalized = normalized[2..];
        return SafeRelativePath(normalized);
    }

    private static string PackagePath(string root, string relativePath)
    {
        var normalized = SafeRelativePath(relativePath);
        var path = Path.GetFullPath(Path.Combine(root, normalized.Replace('/', Path.DirectorySeparatorChar)));
        var prefix = Path.TrimEndingDirectorySeparator(Path.GetFullPath(root)) + Path.DirectorySeparatorChar;
        if (!path.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
        {
            throw new PluginPackageException("An imported Plugin path escapes the package root.");
        }
        return path;
    }

    private static string SafeRelativePath(string value)
    {
        var normalized = value.Replace('\\', '/');
        var parts = normalized.Split('/', StringSplitOptions.None);
        if (normalized.Length == 0 || normalized.Length > 1_024 || normalized.StartsWith('/') ||
            normalized.Contains(':') || parts.Any(part => part.Length == 0 || part is "." or ".." ||
                part.EndsWith(' ') || part.EndsWith('.')))
        {
            throw new PluginPackageException("An imported Plugin path is unsafe on Windows.");
        }
        return normalized;
    }

    private static bool SafeEnvironmentName(string value) =>
        value.Length is > 0 and <= 128 &&
        (char.IsAsciiLetter(value[0]) || value[0] == '_') &&
        value.Skip(1).All(character => char.IsAsciiLetterOrDigit(character) || character == '_');

    private static void RejectReparsePoint(string path)
    {
        if ((File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0)
        {
            throw new PluginPackageException("The imported Plugin contains a reparse point.");
        }
    }
}
