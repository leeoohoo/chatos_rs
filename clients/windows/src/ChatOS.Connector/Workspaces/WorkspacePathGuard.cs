namespace ChatOS.Connector.Workspaces;

public sealed class WorkspacePathGuard
{
    private readonly StringComparison _pathComparison = OperatingSystem.IsWindows()
        ? StringComparison.OrdinalIgnoreCase
        : StringComparison.Ordinal;

    public WorkspacePathGuard(string workspaceRoot)
    {
        if (string.IsNullOrWhiteSpace(workspaceRoot))
        {
            throw new ArgumentException("Workspace root is required.", nameof(workspaceRoot));
        }

        var root = Path.GetFullPath(workspaceRoot);
        if (!Directory.Exists(root))
        {
            throw new Relay.RelayRequestException(404, "Workspace root does not exist.");
        }

        Root = ResolveRoot(root);
    }

    public string Root { get; }

    public string ResolveExisting(string relativePath, bool permitsRoot = true)
    {
        var components = NormalizeComponents(relativePath, permitsRoot);
        var current = Root;
        foreach (var component in components)
        {
            current = Path.Combine(current, component);
            EnsureExistsAndIsNotLink(current);
        }

        var result = Path.GetFullPath(current);
        EnsureContained(result);
        return result;
    }

    public string ResolveWritable(string relativePath)
    {
        var components = NormalizeComponents(relativePath, permitsRoot: false);
        var parentComponents = components[..^1];
        var parent = Root;
        foreach (var component in parentComponents)
        {
            parent = Path.Combine(parent, component);
            EnsureExistsAndIsNotLink(parent);
            if (!Directory.Exists(parent))
            {
                throw new Relay.RelayRequestException(400, "Workspace path parent is not a directory.");
            }
        }

        var target = Path.GetFullPath(Path.Combine(parent, components[^1]));
        EnsureContained(target);
        if (PathExists(target))
        {
            EnsureIsNotLink(target);
        }

        return target;
    }

    public string ResolveMutationTarget(string relativePath)
    {
        var target = ResolveWritable(relativePath);
        if (!PathExists(target))
        {
            throw new Relay.RelayRequestException(404, "Workspace entry does not exist.");
        }

        return target;
    }

    public string RelativePath(string absolutePath)
    {
        var fullPath = Path.GetFullPath(absolutePath);
        EnsureContained(fullPath);
        if (string.Equals(fullPath, Root, _pathComparison))
        {
            return ".";
        }

        return Path.GetRelativePath(Root, fullPath).Replace(Path.DirectorySeparatorChar, '/');
    }

    public void EnsureMoveTargetIsOutsideSource(string source, string target)
    {
        if (!Directory.Exists(source))
        {
            return;
        }

        var sourcePrefix = Path.TrimEndingDirectorySeparator(source) + Path.DirectorySeparatorChar;
        if (target.StartsWith(sourcePrefix, _pathComparison))
        {
            throw new Relay.RelayRequestException(400, "A directory cannot be moved into itself.");
        }
    }

    internal static string[] NormalizeComponents(string path, bool permitsRoot)
    {
        var value = path.Trim();
        if (value.IndexOf('\0') >= 0 ||
            value.StartsWith('/') ||
            value.StartsWith('\\') ||
            Path.IsPathRooted(value))
        {
            throw new Relay.RelayRequestException(400, "Workspace path is unsafe.");
        }

        var components = value
            .Split(['/', '\\'], StringSplitOptions.RemoveEmptyEntries)
            .Where(component => component != ".")
            .ToArray();
        if (components.Any(component =>
                component == ".." ||
                component.Contains(':') ||
                component.IndexOfAny(['<', '>', '"', '|', '?', '*']) >= 0 ||
                component.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0))
        {
            throw new Relay.RelayRequestException(400, "Workspace path is unsafe.");
        }

        if (!permitsRoot && components.Length == 0)
        {
            throw new Relay.RelayRequestException(400, "Workspace root cannot be modified.");
        }

        return components;
    }

    private static string ResolveRoot(string root)
    {
        var info = new DirectoryInfo(root);
        if ((info.Attributes & FileAttributes.ReparsePoint) != 0)
        {
            info = info.ResolveLinkTarget(returnFinalTarget: true) as DirectoryInfo
                ?? throw new Relay.RelayRequestException(400, "Workspace root link is invalid.");
        }

        return Path.TrimEndingDirectorySeparator(Path.GetFullPath(info.FullName));
    }

    private void EnsureContained(string candidate)
    {
        if (string.Equals(candidate, Root, _pathComparison))
        {
            return;
        }

        var rootPrefix = Root + Path.DirectorySeparatorChar;
        if (!candidate.StartsWith(rootPrefix, _pathComparison))
        {
            throw new Relay.RelayRequestException(400, "Workspace path escaped its authorized root.");
        }
    }

    private static void EnsureExistsAndIsNotLink(string path)
    {
        if (!PathExists(path))
        {
            throw new Relay.RelayRequestException(404, "Workspace entry does not exist.");
        }

        EnsureIsNotLink(path);
    }

    private static void EnsureIsNotLink(string path)
    {
        if ((File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0)
        {
            throw new Relay.RelayRequestException(400, "Workspace operations cannot traverse a symbolic link or junction.");
        }
    }

    private static bool PathExists(string path) => File.Exists(path) || Directory.Exists(path);
}
