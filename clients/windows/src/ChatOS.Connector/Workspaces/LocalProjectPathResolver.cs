using ChatOS.Connector.Relay;

namespace ChatOS.Connector.Workspaces;

public sealed record ResolvedLocalProjectPath(
    ConnectorWorkspace Workspace,
    string RelativePath,
    string AbsolutePath,
    string? LogicalPrefix)
{
    public string LogicalPath(string relativePath)
    {
        if (LogicalPrefix is null)
        {
            return relativePath == "."
                ? AbsolutePath
                : Path.Combine(Workspace.AbsoluteRoot, relativePath);
        }

        return relativePath == "."
            ? LogicalPrefix
            : $"{LogicalPrefix}/{relativePath.Replace('\\', '/')}";
    }
}

public interface ILocalProjectPathResolver
{
    ResolvedLocalProjectPath Resolve(string rawPath);
}

public sealed class LocalProjectPathResolver : ILocalProjectPathResolver
{
    private readonly IConnectorWorkspaceContext _context;

    public LocalProjectPathResolver(IConnectorWorkspaceContext context)
    {
        _context = context;
    }

    public ResolvedLocalProjectPath Resolve(string rawPath)
    {
        var value = rawPath?.Trim();
        if (string.IsNullOrWhiteSpace(value))
        {
            throw new RelayRequestException(404, "Project workspace is unavailable.");
        }

        if (Uri.TryCreate(value, UriKind.Absolute, out var uri) &&
            string.Equals(uri.Scheme, "local", StringComparison.OrdinalIgnoreCase) &&
            string.Equals(uri.Host, "connector", StringComparison.OrdinalIgnoreCase))
        {
            return ResolveLogical(uri);
        }

        if (!Path.IsPathFullyQualified(value))
        {
            throw new RelayRequestException(400, "Project path must be a connector path or an absolute local path.");
        }

        var candidate = Path.GetFullPath(value);
        foreach (var workspace in _context.Workspaces)
        {
            WorkspacePathGuard paths;
            try
            {
                paths = new WorkspacePathGuard(workspace.AbsoluteRoot);
            }
            catch (RelayRequestException)
            {
                continue;
            }

            if (!IsInside(candidate, paths.Root))
            {
                continue;
            }

            var relative = paths.RelativePath(candidate);
            var resolved = paths.ResolveExisting(relative);
            return new ResolvedLocalProjectPath(workspace, relative, resolved, null);
        }

        throw new RelayRequestException(404, "Project path is not inside a paired workspace.");
    }

    private ResolvedLocalProjectPath ResolveLogical(Uri uri)
    {
        var parts = uri.AbsolutePath
            .Split('/', StringSplitOptions.RemoveEmptyEntries)
            .Select(Uri.UnescapeDataString)
            .ToArray();
        if (parts.Length < 2 ||
            string.IsNullOrWhiteSpace(_context.DeviceId) ||
            !string.Equals(parts[0], _context.DeviceId, StringComparison.Ordinal))
        {
            throw new RelayRequestException(404, "Project workspace is unavailable on this device.");
        }

        var workspace = _context.Find(parts[1])
            ?? throw new RelayRequestException(404, "Project workspace is unavailable on this device.");
        var relative = parts.Length == 2 ? "." : string.Join('/', parts.Skip(2));
        var paths = new WorkspacePathGuard(workspace.AbsoluteRoot);
        var absolute = paths.ResolveExisting(relative);
        var prefix = $"local://connector/{Uri.EscapeDataString(parts[0])}/{Uri.EscapeDataString(parts[1])}";
        return new ResolvedLocalProjectPath(workspace, paths.RelativePath(absolute), absolute, prefix);
    }

    internal static bool IsInside(string child, string parent)
    {
        var comparison = OperatingSystem.IsWindows()
            ? StringComparison.OrdinalIgnoreCase
            : StringComparison.Ordinal;
        var fullChild = Path.GetFullPath(child);
        var fullParent = Path.TrimEndingDirectorySeparator(Path.GetFullPath(parent));
        return string.Equals(fullChild, fullParent, comparison) ||
            fullChild.StartsWith(fullParent + Path.DirectorySeparatorChar, comparison);
    }
}
