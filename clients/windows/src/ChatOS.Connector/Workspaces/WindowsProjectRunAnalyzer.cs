using System.Text.Json;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Workspaces;

internal sealed record WindowsProjectRunAnalysis(
    IReadOnlyList<ProjectRunTarget> Targets,
    IReadOnlyDictionary<string, IReadOnlyList<ProjectRunToolchainOption>> Toolchains,
    IReadOnlyList<ProjectRunConfigurationFile> ConfigurationFiles);

internal static class WindowsProjectRunAnalyzer
{
    private const int MaximumDirectories = 2_500;
    private const int MaximumDepth = 6;
    private const int MaximumTargets = 100;
    private static readonly HashSet<string> IgnoredDirectories = new(
        [
            ".chatos", ".git", ".idea", ".next", ".venv", ".vscode", "bin",
            "build", "dist", "node_modules", "obj", "target", "venv", "vendor",
        ],
        StringComparer.OrdinalIgnoreCase);
    private static readonly EnumerationOptions TopLevelEnumerationOptions = new()
    {
        IgnoreInaccessible = true,
        RecurseSubdirectories = false,
        ReturnSpecialDirectories = false,
    };

    public static WindowsProjectRunAnalysis Analyze(string root)
    {
        var targets = new List<ProjectRunTarget>();
        var configurations = new List<ProjectRunConfigurationFile>();
        var pending = new Queue<(string Directory, int Depth)>();
        pending.Enqueue((root, 0));
        var visited = 0;
        while (pending.TryDequeue(out var item) &&
               visited++ < MaximumDirectories &&
               targets.Count < MaximumTargets)
        {
            AddPackageJson(root, item.Directory, targets, configurations);
            AddRust(root, item.Directory, targets, configurations);
            AddSwift(root, item.Directory, targets, configurations);
            AddDotNet(root, item.Directory, targets, configurations);
            AddPython(root, item.Directory, targets, configurations);
            if (item.Depth >= MaximumDepth) continue;
            foreach (var directory in AccessibleDirectories(item.Directory))
            {
                try
                {
                    var info = new DirectoryInfo(directory);
                    if ((info.Attributes & FileAttributes.ReparsePoint) != 0 ||
                        IgnoredDirectories.Contains(info.Name))
                    {
                        continue;
                    }
                    pending.Enqueue((directory, item.Depth + 1));
                }
                catch (UnauthorizedAccessException) { }
                catch (IOException) { }
            }
        }
        var requiredKinds = targets.SelectMany(value => value.RequiredToolchains)
            .Distinct(StringComparer.Ordinal)
            .OrderBy(value => value, StringComparer.Ordinal)
            .ToArray();
        var toolchains = requiredKinds.ToDictionary(
            static kind => kind,
            kind => (IReadOnlyList<ProjectRunToolchainOption>)FindToolchains(kind),
            StringComparer.Ordinal);
        return new(
            targets
                .DistinctBy(value => value.Id, StringComparer.Ordinal)
                .OrderByDescending(Priority)
                .ThenBy(value => value.Label, StringComparer.CurrentCultureIgnoreCase)
                .Take(MaximumTargets)
                .ToArray(),
            toolchains,
            configurations
                .DistinctBy(value => value.Path, StringComparer.OrdinalIgnoreCase)
                .OrderBy(value => value.Path, StringComparer.Ordinal)
                .Take(60)
                .ToArray());
    }

    private static void AddPackageJson(
        string root,
        string directory,
        ICollection<ProjectRunTarget> targets,
        ICollection<ProjectRunConfigurationFile> configurations)
    {
        var path = Path.Combine(directory, "package.json");
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
                    script.Name is not ("dev" or "start" or "serve" or "test" or "build") ||
                    script.Name.Any(character =>
                        !char.IsAsciiLetterOrDigit(character) && character is not ':' and not '_' and not '-'))
                {
                    continue;
                }
                targets.Add(new ProjectRunTarget(
                    ScopedId(root, directory, $"npm:{script.Name}"),
                    $"Node · npm {script.Name}",
                    "npm",
                    "javascript",
                    directory,
                    // Script names are restricted above to shell-safe characters. Avoid
                    // quoting them here: cmd.exe receives the generated command through
                    // ProcessStartInfo.ArgumentList, where embedded quotes become literal
                    // npm script-name characters (for example, \"start\").
                    $"npm run {script.Name}",
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
        string directory,
        ICollection<ProjectRunTarget> targets,
        ICollection<ProjectRunConfigurationFile> configurations)
    {
        foreach (var path in AccessibleFiles(directory, "*.csproj"))
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
        string directory,
        ICollection<ProjectRunTarget> targets,
        ICollection<ProjectRunConfigurationFile> configurations)
    {
        var manifest = Path.Combine(directory, "pyproject.toml");
        if (File.Exists(manifest))
        {
            configurations.Add(Configuration(root, manifest, "python", "pyproject.toml"));
        }
        var entrypoint = new[] { "main.py", "app.py" }
            .Select(name => Path.Combine(directory, name))
            .FirstOrDefault(File.Exists);
        if (entrypoint is null) return;
        targets.Add(new ProjectRunTarget(
            ScopedId(root, directory, $"python:{Path.GetFileName(entrypoint)}"),
            $"Python · {Path.GetFileName(entrypoint)}",
            "python",
            "python",
            directory,
            $"python {ShellArgument(entrypoint)}",
            "project",
            false,
            Path.GetFileName(entrypoint),
            File.Exists(manifest) ? "pyproject.toml" : null,
            ["python"]));
    }

    private static void AddRust(
        string root,
        string directory,
        ICollection<ProjectRunTarget> targets,
        ICollection<ProjectRunConfigurationFile> configurations)
    {
        var manifest = Path.Combine(directory, "Cargo.toml");
        if (!File.Exists(manifest)) return;
        configurations.Add(Configuration(root, manifest, "cargo", "Cargo.toml"));
        if (File.Exists(Path.Combine(directory, "src", "main.rs")))
        {
            targets.Add(new ProjectRunTarget(
                ScopedId(root, directory, "cargo:run"),
                "Rust · cargo run",
                "cargo",
                "rust",
                directory,
                "cargo run",
                "Cargo.toml",
                false,
                "src/main.rs",
                Path.GetRelativePath(root, manifest).Replace('\\', '/'),
                ["cargo"]));
        }
        targets.Add(new ProjectRunTarget(
            ScopedId(root, directory, "cargo:test"),
            "Rust · cargo test",
            "cargo",
            "rust",
            directory,
            "cargo test",
            "Cargo.toml",
            false,
            null,
            Path.GetRelativePath(root, manifest).Replace('\\', '/'),
            ["cargo"]));
    }

    private static void AddSwift(
        string root,
        string directory,
        ICollection<ProjectRunTarget> targets,
        ICollection<ProjectRunConfigurationFile> configurations)
    {
        AddManifestTarget(root, directory, "Package.swift", "swift:run", "Swift · swift run",
            "swift", "swift", "swift run", ["swift"], targets, configurations);
        AddManifestTarget(root, directory, "Package.swift", "swift:test", "Swift · swift test",
            "swift", "swift", "swift test", ["swift"], targets, configurations,
            addConfiguration: false);
    }

    private static void AddManifestTarget(
        string root,
        string directory,
        string manifestName,
        string id,
        string label,
        string kind,
        string language,
        string command,
        IReadOnlyList<string> toolchains,
        ICollection<ProjectRunTarget> targets,
        ICollection<ProjectRunConfigurationFile> configurations,
        bool addConfiguration = true)
    {
        var path = Path.Combine(directory, manifestName);
        if (!File.Exists(path)) return;
        if (addConfiguration) configurations.Add(Configuration(root, path, kind, manifestName));
        targets.Add(new ProjectRunTarget(
            ScopedId(root, directory, id),
            label,
            kind,
            language,
            directory,
            command,
            manifestName,
            false,
            null,
            Path.GetRelativePath(root, path).Replace('\\', '/'),
            toolchains));
    }

    private static string ScopedId(string root, string directory, string id)
    {
        var relative = Path.GetRelativePath(root, directory).Replace('\\', '/');
        return relative == "." ? id : $"{id}:{relative}";
    }

    private static int Priority(ProjectRunTarget target)
    {
        var command = target.Command.ToLowerInvariant();
        if (command.Contains("npm run dev", StringComparison.Ordinal) ||
            command.Contains("npm run start", StringComparison.Ordinal)) return 100;
        if (command == "cargo run" || command == "swift run") return 84;
        if (command.Contains("main.py", StringComparison.Ordinal) ||
            command.Contains("app.py", StringComparison.Ordinal)) return 88;
        if (command.Contains("test", StringComparison.Ordinal)) return 40;
        return 70;
    }

    private static IReadOnlyList<string> AccessibleFiles(string directory, string pattern)
    {
        try
        {
            return Directory.GetFiles(directory, pattern, TopLevelEnumerationOptions);
        }
        catch (UnauthorizedAccessException)
        {
            return [];
        }
        catch (IOException)
        {
            return [];
        }
    }

    private static IReadOnlyList<string> AccessibleDirectories(string directory)
    {
        try
        {
            return Directory.GetDirectories(directory, "*", TopLevelEnumerationOptions);
        }
        catch (UnauthorizedAccessException)
        {
            return [];
        }
        catch (IOException)
        {
            return [];
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
