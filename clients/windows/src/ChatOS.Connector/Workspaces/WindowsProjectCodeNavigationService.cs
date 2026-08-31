using System.Text.Json;
using ChatOS.Connector.Relay;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Workspaces;

public sealed class WindowsProjectCodeNavigationService : IProjectCodeNavigationService
{
    private readonly ILocalProjectPathResolver _paths;
    private readonly object _cacheGate = new();
    private readonly Dictionary<SearchCacheKey, SearchCacheEntry> _cache = [];

    public WindowsProjectCodeNavigationService(ILocalProjectPathResolver paths)
    {
        _paths = paths;
    }

    public Task<ProjectCodeNavigationResult> DefinitionAsync(
        string projectRoot,
        string filePath,
        int line,
        int column,
        CancellationToken cancellationToken = default) =>
        NavigateAsync(projectRoot, filePath, line, column, definitions: true, cancellationToken);

    public Task<ProjectCodeNavigationResult> ReferencesAsync(
        string projectRoot,
        string filePath,
        int line,
        int column,
        CancellationToken cancellationToken = default) =>
        NavigateAsync(projectRoot, filePath, line, column, definitions: false, cancellationToken);

    private async Task<ProjectCodeNavigationResult> NavigateAsync(
        string projectRoot,
        string filePath,
        int line,
        int column,
        bool definitions,
        CancellationToken cancellationToken)
    {
        var context = await ContextAsync(projectRoot, filePath, cancellationToken).ConfigureAwait(false);
        var token = TokenAt(line, column, context.Content);
        if (token is null)
        {
            return Result(context, definitions ? "heuristic" : "text-search", null, []);
        }

        var hits = await SearchAsync(context, token, definitions ? 240 : 500, cancellationToken)
            .ConfigureAwait(false);
        var locations = new List<ProjectCodeNavigationLocation>();
        foreach (var hit in hits)
        {
            var tokenColumn = TokenColumn(hit.Text, token);
            if (tokenColumn is null)
            {
                continue;
            }

            var score = definitions
                ? DefinitionScore(context, hit, token, line)
                : hit.RelativePath == context.FileProjectRelative ? 1.5 : 1;
            if (definitions && score < 2)
            {
                continue;
            }

            var location = new ProjectCodeNavigationLocation(
                context.Root.LogicalPath(hit.WorkspaceRelativePath),
                hit.RelativePath,
                hit.Line,
                tokenColumn.Value,
                hit.Line,
                tokenColumn.Value + Math.Max(0, token.Length - 1),
                hit.Text,
                score);
            if (!IsRequestLocation(location, context, line, column))
            {
                locations.Add(location);
            }
        }

        var ordered = definitions
            ? locations.OrderByDescending(static value => value.Score)
                .ThenBy(static value => value.RelativePath, StringComparer.OrdinalIgnoreCase)
                .ThenBy(static value => value.Line)
                .Take(20)
            : locations.OrderBy(static value => value.RelativePath, StringComparer.OrdinalIgnoreCase)
                .ThenBy(static value => value.Line)
                .ThenBy(static value => value.Column)
                .Take(200);
        return Result(
            context,
            definitions ? "heuristic" : "text-search",
            token,
            ordered.ToArray());
    }

    private Task<NavigationContext> ContextAsync(
        string projectRoot,
        string filePath,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var root = _paths.Resolve(projectRoot);
        var file = _paths.Resolve(filePath);
        if (root.Workspace.Id != file.Workspace.Id ||
            !LocalProjectPathResolver.IsInside(file.AbsolutePath, root.AbsolutePath) ||
            string.Equals(file.AbsolutePath, root.AbsolutePath, StringComparison.OrdinalIgnoreCase))
        {
            throw new RelayRequestException(400, "当前文件不在项目目录内，无法执行代码导航。");
        }

        var read = new WorkspaceFilesystem(file.Workspace).Read(file.RelativePath);
        if (read.GetProperty("is_binary").GetBoolean())
        {
            throw new RelayRequestException(400, "二进制文件不支持代码导航。");
        }

        var projectRelative = ProjectRelativePath(file.RelativePath, root.RelativePath)
            ?? throw new RelayRequestException(400, "当前文件不在项目目录内，无法执行代码导航。");
        return Task.FromResult(new NavigationContext(
            root,
            projectRelative,
            read.GetProperty("content").GetString() ?? string.Empty,
            Language(file.AbsolutePath)));
    }

    private Task<IReadOnlyList<SearchHit>> SearchAsync(
        NavigationContext context,
        string token,
        int limit,
        CancellationToken cancellationToken)
    {
        var key = new SearchCacheKey(context.Root.Workspace.Id, context.Root.RelativePath, token);
        lock (_cacheGate)
        {
            if (_cache.TryGetValue(key, out var cached) &&
                DateTimeOffset.UtcNow - cached.CreatedAt < TimeSpan.FromSeconds(8))
            {
                return Task.FromResult<IReadOnlyList<SearchHit>>(cached.Hits.Take(limit).ToArray());
            }
        }

        return Task.Run<IReadOnlyList<SearchHit>>(() =>
        {
            var value = new WorkspaceFilesystem(context.Root.Workspace).SearchContent(
                context.Root.RelativePath,
                token,
                500,
                cancellationToken);
            var hits = new List<SearchHit>();
            if (value.TryGetProperty("matches", out var matches))
            {
                foreach (var item in matches.EnumerateArray())
                {
                    cancellationToken.ThrowIfCancellationRequested();
                    var workspaceRelative = item.GetProperty("path").GetString() ?? string.Empty;
                    var relative = ProjectRelativePath(workspaceRelative, context.Root.RelativePath);
                    if (relative is null)
                    {
                        continue;
                    }

                    hits.Add(new SearchHit(
                        workspaceRelative,
                        relative,
                        item.GetProperty("line").GetInt32(),
                        item.GetProperty("text").GetString() ?? string.Empty));
                }
            }

            lock (_cacheGate)
            {
                _cache[key] = new SearchCacheEntry(DateTimeOffset.UtcNow, hits);
                foreach (var oldKey in _cache.OrderBy(static pair => pair.Value.CreatedAt)
                             .Take(Math.Max(0, _cache.Count - 24))
                             .Select(static pair => pair.Key)
                             .ToArray())
                {
                    _cache.Remove(oldKey);
                }
            }

            return hits.Take(limit).ToArray();
        }, cancellationToken);
    }

    internal static string? TokenAt(int line, int column, string content)
    {
        var lines = content.Split('\n');
        if (line <= 0 || line > lines.Length || lines[line - 1].Length == 0)
        {
            return null;
        }

        var value = lines[line - 1].TrimEnd('\r');
        var index = Math.Clamp(column - 1, 0, value.Length - 1);
        if (!IsTokenCharacter(value[index]) && index > 0 && IsTokenCharacter(value[index - 1])) index--;
        if (!IsTokenCharacter(value[index])) return null;
        var start = index;
        var end = index;
        while (start > 0 && IsTokenCharacter(value[start - 1])) start--;
        while (end + 1 < value.Length && IsTokenCharacter(value[end + 1])) end++;
        return value[start..(end + 1)];
    }

    internal static int? TokenColumn(string line, string token)
    {
        var start = 0;
        while (start <= line.Length - token.Length)
        {
            var index = line.IndexOf(token, start, StringComparison.Ordinal);
            if (index < 0) return null;
            var before = index == 0 || !IsTokenCharacter(line[index - 1]);
            var afterIndex = index + token.Length;
            var after = afterIndex == line.Length || !IsTokenCharacter(line[afterIndex]);
            if (before && after) return index + 1;
            start = index + 1;
        }

        return null;
    }

    private static double DefinitionScore(NavigationContext context, SearchHit hit, string token, int line)
    {
        var lower = hit.Text.ToLowerInvariant();
        var tokenLower = token.ToLowerInvariant();
        var score = hit.RelativePath == context.FileProjectRelative ? 1.5 : 0;
        if (string.Equals(Path.GetFileNameWithoutExtension(hit.RelativePath), token, StringComparison.OrdinalIgnoreCase)) score += 4;
        if (hit.RelativePath == context.FileProjectRelative && hit.Line == line) score -= 3;
        var patterns = new[]
        {
            $"class {tokenLower}", $"interface {tokenLower}", $"enum {tokenLower}",
            $"struct {tokenLower}", $"type {tokenLower} ", $"func {tokenLower}",
            $"fn {tokenLower}", $"def {tokenLower}", $"function {tokenLower}",
            $"const {tokenLower} =", $"let {tokenLower} =", $"var {tokenLower} =",
            $"const {tokenLower}:", $"let {tokenLower}:", $"var {tokenLower}:",
        };
        score += patterns.Count(lower.Contains) * 2;
        if (lower.TrimStart().StartsWith(tokenLower, StringComparison.Ordinal)) score += 1;
        return score;
    }

    private static bool IsRequestLocation(
        ProjectCodeNavigationLocation location,
        NavigationContext context,
        int line,
        int column) =>
        location.RelativePath == context.FileProjectRelative &&
        location.Line == line &&
        location.Column <= column &&
        location.EndColumn >= column;

    private static bool IsTokenCharacter(char character) =>
        char.IsLetterOrDigit(character) || character is '_' or '$';

    private static string? ProjectRelativePath(string workspaceRelative, string rootRelative)
    {
        if (rootRelative is "." or "") return workspaceRelative;
        if (workspaceRelative == rootRelative) return string.Empty;
        var prefix = rootRelative + "/";
        return workspaceRelative.StartsWith(prefix, StringComparison.Ordinal)
            ? workspaceRelative[prefix.Length..]
            : null;
    }

    private static string Language(string file) => Path.GetExtension(file).ToLowerInvariant() switch
    {
        ".cs" => "csharp", ".java" => "java", ".ts" or ".tsx" or ".mts" or ".cts" => "typescript",
        ".js" or ".jsx" or ".mjs" or ".cjs" => "javascript", ".rs" => "rust", ".go" => "go",
        ".py" => "python", ".kt" or ".kts" => "kotlin", ".swift" => "swift", ".php" => "php",
        ".rb" => "ruby", ".cpp" or ".cc" or ".cxx" or ".hpp" or ".hh" or ".h" or ".hxx" => "cpp",
        ".c" => "c", ".sh" or ".bash" or ".zsh" => "shell", ".json" => "json",
        ".yaml" or ".yml" => "yaml", ".toml" => "toml", ".md" or ".markdown" => "markdown",
        _ => Path.GetFileName(file).Equals("Dockerfile", StringComparison.OrdinalIgnoreCase) ? "dockerfile" : "unknown",
    };

    private static ProjectCodeNavigationResult Result(
        NavigationContext context,
        string mode,
        string? token,
        IReadOnlyList<ProjectCodeNavigationLocation> locations) =>
        new("windows-native-fallback", context.Language, mode, token, locations);

    private sealed record NavigationContext(
        ResolvedLocalProjectPath Root,
        string FileProjectRelative,
        string Content,
        string Language);

    private sealed record SearchHit(string WorkspaceRelativePath, string RelativePath, int Line, string Text);
    private sealed record SearchCacheKey(string WorkspaceId, string RootPath, string Token);
    private sealed record SearchCacheEntry(DateTimeOffset CreatedAt, IReadOnlyList<SearchHit> Hits);
}
