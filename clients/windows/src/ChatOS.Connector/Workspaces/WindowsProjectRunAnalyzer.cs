using System.Text.Json;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Workspaces;

internal sealed record WindowsProjectRunAnalysis(
    IReadOnlyList<ProjectRunTarget> Targets,
    IReadOnlyDictionary<string, IReadOnlyList<ProjectRunToolchainOption>> Toolchains,
    IReadOnlyList<ProjectRunConfigurationFile> ConfigurationFiles);

internal static class WindowsProjectRunAnalyzer
{
    public static WindowsProjectRunAnalysis Analyze(string root)
    {
        var targets = new List<ProjectRunTarget>();
        var configurations = new List<ProjectRunConfigurationFile>();
        AddPackageJson(root, targets, configurations);
        AddManifestTarget(
            root,
            "Cargo.toml",
            "cargo:run",
            "Cargo run",
            "cargo",
            "rust",
            "cargo run",
            ["cargo"],
            targets,
            configurations);
        AddManifestTarget(
            root,
            "Package.swift",
            "swift:run",
            "Swift run",
            "swift",
            "swift",
            "swift run",
            ["swift"],
            targets,
            configurations);
        AddDotNet(root, targets, configurations);
        AddPython(root, targets, configurations);
        var requiredKinds = targets.SelectMany(value => value.RequiredToolchains)
            .Distinct(StringComparer.Ordinal)
            .OrderBy(value => value, StringComparer.Ordinal)
            .ToArray();
        var toolchains = requiredKinds.ToDictionary(
            static kind => kind,
            kind => (IReadOnlyList<ProjectRunToolchainOption>)FindToolchains(kind),
            StringComparer.Ordinal);
        return new(
            targets.OrderBy(value => value.Label, StringComparer.CurrentCultureIgnoreCase).ToArray(),
            toolchains,
            configurations.OrderBy(value => value.Path, StringComparer.Ordinal).ToArray());
    }

    private static void AddPackageJson(
        string root,
        ICollection<ProjectRunTarget> targets,
        ICollection<ProjectRunConfigurationFile> configurations)
    {
        var path = Path.Combine(root, "package.json");
        if (!File.Exists(path)) return;
        configurations.Add(Configuration(root, path, "package", "package.json"));
        try
        {
            using var document = JsonDocument.Parse(File.ReadAllText(path));
            if (!document.RootElement.TryGetProperty("scripts", out var scripts) ||
                scripts.ValueKind != JsonValueKind.Object)
            {
                return;
            }
            foreach (var script in scripts.EnumerateObject())
            {
                if (script.Value.ValueKind != JsonValueKind.String ||
                    script.Name.Any(character =>
                        !char.IsAsciiLetterOrDigit(character) && character is not ':' and not '_' and not '-'))
                {
                    continue;
                }
                targets.Add(new ProjectRunTarget(
                    $"npm:{script.Name}",
                    $"npm · {script.Name}",
                    "npm",
                    "javascript",
                    root,
                    $"npm run {ShellArgument(script.Name)}",
                    "package.json",
                    false,
                    null,
                    "package.json",
                    ["node", "npm"]));
            }
        }
        catch (JsonException)
        {
        }
    }

    private static void AddDotNet(
        string root,
        ICollection<ProjectRunTarget> targets,
        ICollection<ProjectRunConfigurationFile> configurations)
    {
        foreach (var path in TopLevelFiles(root, "*.csproj"))
        {
            var relative = Path.GetRelativePath(root, path).Replace('\\', '/');
            configurations.Add(Configuration(root, path, "dotnet", Path.GetFileName(path)));
            targets.Add(new ProjectRunTarget(
                $"dotnet:{relative}",
                $".NET · {Path.GetFileNameWithoutExtension(path)}",
                "dotnet",
                "csharp",
                Path.GetDirectoryName(path) ?? root,
                $"dotnet run --project {ShellArgument(path)}",
                Path.GetFileName(path),
                false,
                null,
                relative,
                ["dotnet"]));
        }
    }

    private static void AddPython(
        string root,
        ICollection<ProjectRunTarget> targets,
        ICollection<ProjectRunConfigurationFile> configurations)
    {
        var manifest = Path.Combine(root, "pyproject.toml");
        if (File.Exists(manifest))
        {
            configurations.Add(Configuration(root, manifest, "python", "pyproject.toml"));
        }
        var entrypoint = new[] { "main.py", "app.py" }
            .Select(name => Path.Combine(root, name))
            .FirstOrDefault(File.Exists);
        if (entrypoint is null) return;
        targets.Add(new ProjectRunTarget(
            $"python:{Path.GetFileName(entrypoint)}",
            $"Python · {Path.GetFileName(entrypoint)}",
            "python",
            "python",
            root,
            $"python {ShellArgument(entrypoint)}",
            "project",
            false,
            Path.GetFileName(entrypoint),
            File.Exists(manifest) ? "pyproject.toml" : null,
            ["python"]));
    }

    private static void AddManifestTarget(
        string root,
        string manifestName,
        string id,
        string label,
        string kind,
        string language,
        string command,
        IReadOnlyList<string> toolchains,
        ICollection<ProjectRunTarget> targets,
        ICollection<ProjectRunConfigurationFile> configurations)
    {
        var path = Path.Combine(root, manifestName);
        if (!File.Exists(path)) return;
        configurations.Add(Configuration(root, path, kind, manifestName));
        targets.Add(new ProjectRunTarget(
            id,
            label,
            kind,
            language,
            root,
            command,
            manifestName,
            false,
            null,
            manifestName,
            toolchains));
    }

    private static IEnumerable<string> TopLevelFiles(string root, string pattern)
    {
        foreach (var path in Directory.EnumerateFiles(root, pattern, SearchOption.TopDirectoryOnly))
        {
            yield return path;
        }
        foreach (var directory in Directory.EnumerateDirectories(root))
        {
            var info = new DirectoryInfo(directory);
            if ((info.Attributes & FileAttributes.ReparsePoint) != 0 ||
                info.Name is "bin" or "obj" or "node_modules" or ".git")
            {
                continue;
            }
            foreach (var path in Directory.EnumerateFiles(directory, pattern, SearchOption.TopDirectoryOnly))
            {
                yield return path;
            }
        }
    }

    private static ProjectRunConfigurationFile Configuration(
        string root,
        string path,
        string kind,
        string label)
    {
        var info = new FileInfo(path);
        var preview = info.Length <= 32 * 1024 ? File.ReadAllText(path) : null;
        return new(kind, label, Path.GetRelativePath(root, path).Replace('\\', '/'), preview, "project");
    }

    private static ProjectRunToolchainOption[] FindToolchains(string kind)
    {
        var executable = kind switch
        {
            "node" => "node",
            "npm" => "npm",
            "cargo" => "cargo",
            "dotnet" => "dotnet",
            "python" => "python",
            "swift" => "swift",
            _ => kind,
        };
        var path = FindExecutable(executable);
        return path is null
            ? []
            : [new ProjectRunToolchainOption(path, kind, executable, null, path, "path", true)];
    }

    private static string? FindExecutable(string name)
    {
        var extensions = OperatingSystem.IsWindows()
            ? (Environment.GetEnvironmentVariable("PATHEXT") ?? ".EXE;.CMD;.BAT")
                .Split(';', StringSplitOptions.RemoveEmptyEntries)
            : [string.Empty];
        foreach (var directory in (Environment.GetEnvironmentVariable("PATH") ?? string.Empty)
            .Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries))
        {
            foreach (var extension in extensions)
            {
                var path = Path.Combine(directory.Trim('"'), name + extension.ToLowerInvariant());
                if (File.Exists(path)) return Path.GetFullPath(path);
            }
        }
        return null;
    }

    private static string ShellArgument(string value) =>
        "\"" + value.Replace("\"", "\\\"") + "\"";
}
