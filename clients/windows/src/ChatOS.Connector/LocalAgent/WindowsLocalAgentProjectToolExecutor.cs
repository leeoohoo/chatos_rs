using System.Text.Json;
using System.Text;
using ChatOS.Connector.Terminal;
using ChatOS.Connector.Workspaces;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsLocalAgentProjectContext(
    string ConversationId,
    LocalProjectRecord Project,
    ConnectorWorkspace Workspace,
    string ProjectRoot,
    WorkspaceFilesystem Files,
    WindowsLocalAgentTaskToolAuthorization Authorization,
    string? RemoteConnectionId);

internal sealed record WindowsLocalAgentTaskToolAuthorization(
    bool RequiresExecution,
    IReadOnlySet<string> EnabledBuiltinKinds,
    IReadOnlySet<string> PluginKeys,
    bool IsLegacyUnrestricted)
{
    public bool Allows(string toolName)
    {
        if (IsLegacyUnrestricted) return true;
        if (toolName is "project_list" or "project_read" or "project_search")
        {
            return EnabledBuiltinKinds.Contains("CodeMaintainerRead");
        }
        if (toolName == "project_write")
        {
            return RequiresExecution && EnabledBuiltinKinds.Contains("CodeMaintainerWrite");
        }
        if (toolName == "terminal_exec")
        {
            return RequiresExecution && EnabledBuiltinKinds.Contains("TerminalController");
        }
        if (toolName.StartsWith(
                WindowsLocalAgentCapabilityCatalog.RemoteConnectionToolPrefix,
                StringComparison.Ordinal))
        {
            return EnabledBuiltinKinds.Contains("RemoteConnectionController");
        }
        return WindowsLocalAgentCapabilityCatalog.PluginToolNames.Contains(toolName) &&
            PluginKeys.Count > 0;
    }

    public static WindowsLocalAgentTaskToolAuthorization Resolve(JsonElement input)
    {
        if (input.ValueKind != JsonValueKind.Object ||
            !input.TryGetProperty("tool_options", out var options))
        {
            return new(true, new HashSet<string>(StringComparer.Ordinal),
                new HashSet<string>(StringComparer.Ordinal), true);
        }
        if (options.ValueKind != JsonValueKind.Object ||
            !options.TryGetProperty("requires_execution", out var requiresExecution) ||
            requiresExecution.ValueKind is not (JsonValueKind.True or JsonValueKind.False) ||
            !options.TryGetProperty("enabled_builtin_kinds", out var rawKinds) ||
            rawKinds.ValueKind != JsonValueKind.Array)
        {
            throw new InvalidOperationException("Invalid Local Agent Task tool scope.");
        }
        var kinds = StringSet(rawKinds, "enabled_builtin_kinds");
        var pluginKeys = options.TryGetProperty("plugin_hints", out var rawHints)
            ? ParsePluginKeys(rawHints)
            : new HashSet<string>(StringComparer.Ordinal);
        return new(requiresExecution.GetBoolean(), kinds, pluginKeys, false);
    }

    private static HashSet<string> StringSet(JsonElement values, string field)
    {
        var output = new HashSet<string>(StringComparer.Ordinal);
        foreach (var value in values.EnumerateArray())
        {
            if (value.ValueKind != JsonValueKind.String ||
                string.IsNullOrWhiteSpace(value.GetString()) ||
                !output.Add(value.GetString()!))
            {
                throw new InvalidOperationException(
                    $"Invalid Local Agent Task tool scope: {field}.");
            }
        }
        return output;
    }

    private static HashSet<string> ParsePluginKeys(JsonElement hints)
    {
        if (hints.ValueKind != JsonValueKind.Array)
        {
            throw new InvalidOperationException("Invalid Local Agent Task Plugin scope.");
        }
        var output = new HashSet<string>(StringComparer.Ordinal);
        foreach (var hint in hints.EnumerateArray())
        {
            if (hint.ValueKind != JsonValueKind.Object ||
                !hint.TryGetProperty("plugin_key", out var rawKey) ||
                rawKey.ValueKind != JsonValueKind.String ||
                string.IsNullOrWhiteSpace(rawKey.GetString()) ||
                !output.Add(rawKey.GetString()!.Trim()))
            {
                throw new InvalidOperationException("Invalid Local Agent Task Plugin scope.");
            }
        }
        return output;
    }
}

internal sealed class WindowsLocalAgentProjectContextResolver(
    ILocalAgentHostClient host,
    WindowsLocalAgentConversationClient conversations,
    IProjectRegistry projects,
    IConnectorWorkspaceContext workspaces)
{
    public async Task<WindowsLocalAgentProjectContext> ResolveAsync(
        string ownerUserId,
        string runId,
        CancellationToken cancellationToken)
    {
        var runResult = await host.SendAsync<GetLocalRunCommand, GetLocalRunResult>(
            new("get_run", ownerUserId, runId), cancellationToken).ConfigureAwait(false);
        if (runResult.Type != "run" || runResult.Run.OwnerUserId != ownerUserId)
        {
            throw new InvalidOperationException("Invalid Local Agent Run context.");
        }

        var conversationId = String(runResult.Run.Input, "source_conversation_id")
            ?? String(runResult.Run.Input, "conversation_id")
            ?? throw new InvalidOperationException("Invalid Local Agent conversation context.");
        var detail = await conversations.GetAsync(ownerUserId, conversationId, cancellationToken)
            .ConfigureAwait(false);
        var resource = detail.Conversation.Resource;
        if (detail.Conversation.OwnerUserId != ownerUserId || resource is null ||
            resource.Kind != WindowsLocalAgentWorkspaceService.ProjectResourceKind)
        {
            throw new InvalidOperationException("The Local Agent project is unavailable.");
        }

        var project = await projects.GetAsync(ownerUserId, resource.ResourceId, cancellationToken)
            .ConfigureAwait(false);
        if (project is null || project.Status != LocalProjectStatus.Active)
        {
            throw new InvalidOperationException("The Local Agent project is unavailable.");
        }

        var workspace = workspaces.Find(project.Draft.WorkspaceId)
            ?? throw new InvalidOperationException(
                "The Local Agent project workspace is unavailable on this device.");
        var relativeRoot = string.IsNullOrEmpty(project.Draft.RelativeRoot)
            ? "."
            : project.Draft.RelativeRoot;
        var projectRoot = new WorkspacePathGuard(workspace.AbsoluteRoot)
            .ResolveExisting(relativeRoot);
        if (!Directory.Exists(projectRoot))
        {
            throw new InvalidOperationException("The Local Agent project directory is unavailable.");
        }

        var projectWorkspace = new ConnectorWorkspace(
            project.Id,
            project.Draft.Name,
            projectRoot,
            workspace.Fingerprint,
            workspace.ProjectConfigTrusted,
            workspace.ProjectConfigTrustStale);
        return new WindowsLocalAgentProjectContext(
            conversationId,
            project,
            workspace,
            projectRoot,
            new WorkspaceFilesystem(projectWorkspace),
            WindowsLocalAgentTaskToolAuthorization.Resolve(runResult.Run.Input),
            String(runResult.Run.Input, "remote_connection_id"));
    }

    private static string? String(JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var property) &&
        property.ValueKind == JsonValueKind.String &&
        !string.IsNullOrWhiteSpace(property.GetString())
            ? property.GetString()
            : null;
}

public sealed class WindowsLocalAgentProjectToolExecutor
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);
    private readonly WindowsLocalAgentProjectContextResolver _contexts;
    private readonly ITerminalCommandExecutor _commands;
    private readonly IRemoteConnectionService _remoteConnections;
    private readonly IRemoteSftpService _remoteFiles;
    private readonly IRemoteTerminalCommandService _remoteCommands;

    public WindowsLocalAgentProjectToolExecutor(
        ILocalAgentHostClient host,
        WindowsLocalAgentConversationClient conversations,
        IProjectRegistry projects,
        IConnectorWorkspaceContext workspaces,
        ITerminalCommandExecutor commands,
        IRemoteConnectionService remoteConnections,
        IRemoteSftpService remoteFiles,
        IRemoteTerminalCommandService remoteCommands)
    {
        _contexts = new WindowsLocalAgentProjectContextResolver(
            host, conversations, projects, workspaces);
        _commands = commands;
        _remoteConnections = remoteConnections;
        _remoteFiles = remoteFiles;
        _remoteCommands = remoteCommands;
    }

    internal Task<WindowsLocalAgentProjectContext> ResolveContextAsync(
        string ownerUserId,
        string runId,
        CancellationToken cancellationToken) =>
        _contexts.ResolveAsync(ownerUserId, runId, cancellationToken);

    internal async Task<JsonElement> ExecuteAsync(
        string ownerUserId,
        WindowsLocalToolInvocation invocation,
        CancellationToken cancellationToken)
    {
        if (!WindowsLocalAgentCapabilityCatalog.ProjectToolNames.Contains(
                invocation.ToolName) || invocation.Arguments.ValueKind != JsonValueKind.Object)
        {
            throw new InvalidOperationException("Unsupported Local Agent project tool.");
        }
        if ((invocation.ToolName is "project_write" or "terminal_exec") &&
            (!invocation.RequiresApproval || invocation.ApprovalStatus != "approved"))
        {
            throw new InvalidOperationException(
                "The Local Agent project change requires Host approval.");
        }
        if (invocation.ToolName is
                "remote_connection_controller_run_command" or
                "remote_connection_controller_upload_file" &&
            (!invocation.RequiresApproval || invocation.ApprovalStatus != "approved"))
        {
            throw new InvalidOperationException(
                "The Local Agent remote operation requires Host approval.");
        }

        var context = await _contexts.ResolveAsync(
            ownerUserId, invocation.RunId, cancellationToken).ConfigureAwait(false);
        if (!context.Authorization.Allows(invocation.ToolName))
        {
            throw new InvalidOperationException(
                "The Local Agent Task capability was not selected.");
        }
        if (invocation.ToolName.StartsWith(
                WindowsLocalAgentCapabilityCatalog.RemoteConnectionToolPrefix,
                StringComparison.Ordinal))
        {
            var remoteConnectionId = context.RemoteConnectionId;
            if (string.IsNullOrWhiteSpace(remoteConnectionId))
            {
                throw new InvalidOperationException(
                    "The Local Agent Task has no program-bound remote connection.");
            }
            return await ExecuteRemoteAsync(
                remoteConnectionId,
                invocation.ToolName[WindowsLocalAgentCapabilityCatalog.RemoteConnectionToolPrefix.Length..],
                invocation.Arguments,
                cancellationToken).ConfigureAwait(false);
        }
        return invocation.ToolName switch
        {
            "project_list" => context.Files.List(
                OptionalString(invocation.Arguments, "path") ?? ".",
                OptionalBool(invocation.Arguments, "include_files") ?? true),
            "project_read" => context.Files.Read(
                RequiredString(invocation.Arguments, "path")),
            "project_search" => Search(context.Files, invocation.Arguments, cancellationToken),
            "project_write" => context.Files.Write(
                RequiredString(invocation.Arguments, "path"),
                RequiredString(invocation.Arguments, "content"),
                OptionalBool(invocation.Arguments, "create_only") ?? false),
            "terminal_exec" => await ExecuteCommandAsync(
                context, invocation.Arguments, cancellationToken).ConfigureAwait(false),
            _ => throw new InvalidOperationException("Unsupported Local Agent project tool."),
        };
    }

    private async Task<JsonElement> ExecuteRemoteAsync(
        string connectionId,
        string toolName,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        switch (toolName)
        {
            case "test_connection":
            {
                var result = await _remoteConnections.TestSavedAsync(
                    connectionId, null, cancellationToken).ConfigureAwait(false);
                return JsonSerializer.SerializeToElement(new
                {
                    success = result.Success,
                    message = result.Message,
                }, JsonOptions);
            }
            case "run_command":
            {
                var command = RequiredString(arguments, "command");
                if (IsDangerousRemoteCommand(command) &&
                    OptionalBool(arguments, "allow_dangerous") != true)
                {
                    throw new InvalidOperationException(
                        "The dangerous remote command requires allow_dangerous=true.");
                }
                var result = await _remoteCommands.ExecuteAsync(
                    connectionId,
                    command,
                    OptionalString(arguments, "working_directory") ?? "~",
                    null,
                    cancellationToken).ConfigureAwait(false);
                var limit = Math.Clamp(OptionalInt(arguments, "max_output_chars") ?? 20_000, 1, 20_000);
                return JsonSerializer.SerializeToElement(new
                {
                    command,
                    exit_code = result.ExitCode,
                    success = result.ExitCode == 0,
                    stdout = LimitCharacters(result.Output, limit),
                    stderr = LimitCharacters(result.Error, limit),
                    working_directory = result.WorkingDirectory,
                }, JsonOptions);
            }
            case "list_directory":
            {
                var path = OptionalString(arguments, "path") ?? ".";
                var limit = Math.Clamp(OptionalInt(arguments, "limit") ?? 200, 1, 1_000);
                var entries = await _remoteFiles.ListAsync(
                    connectionId, path, null, cancellationToken).ConfigureAwait(false);
                return JsonSerializer.SerializeToElement(new
                {
                    path,
                    entries = entries.Take(limit).Select(entry => new
                    {
                        name = entry.Name,
                        path = entry.FullPath,
                        type = entry.IsDirectory ? "directory" : entry.IsSymbolicLink ? "symlink" : "file",
                        is_directory = entry.IsDirectory,
                        size_bytes = entry.Size,
                        modified_at = entry.LastModifiedAt,
                    }).ToArray(),
                    count = Math.Min(entries.Count, limit),
                    truncated = entries.Count > limit,
                }, JsonOptions);
            }
            case "read_file":
            {
                var path = RequiredString(arguments, "path");
                var limit = Math.Clamp(OptionalInt(arguments, "max_bytes") ?? 256 * 1_024, 1, 256 * 1_024);
                var content = await _remoteFiles.ReadTextAsync(
                    connectionId, path, null, cancellationToken).ConfigureAwait(false);
                var bytes = Encoding.UTF8.GetBytes(content);
                if (bytes.Length > limit)
                {
                    throw new InvalidOperationException("The remote file exceeds the selected read limit.");
                }
                return JsonSerializer.SerializeToElement(new
                {
                    path,
                    encoding = "text",
                    size_bytes = bytes.Length,
                    content,
                }, JsonOptions);
            }
            case "download_file":
            {
                var path = RequiredString(arguments, "path");
                var encoding = OptionalString(arguments, "encoding") ?? "text";
                if (encoding is not ("text" or "base64"))
                {
                    throw new InvalidOperationException("encoding must be text or base64.");
                }
                var limit = Math.Clamp(OptionalInt(arguments, "max_bytes") ?? 256 * 1_024, 1, 256 * 1_024);
                await using var output = new BoundedMemoryStream(limit);
                await _remoteFiles.DownloadAsync(
                    connectionId, path, output, null, cancellationToken).ConfigureAwait(false);
                var bytes = output.ToArray();
                string content;
                if (encoding == "base64")
                {
                    content = Convert.ToBase64String(bytes);
                }
                else
                {
                    content = new UTF8Encoding(false, true).GetString(bytes);
                }
                return JsonSerializer.SerializeToElement(new
                {
                    path,
                    encoding,
                    size_bytes = bytes.Length,
                    content,
                }, JsonOptions);
            }
            case "upload_file":
            {
                var path = RequiredString(arguments, "path");
                var content = RequiredStringAllowEmpty(arguments, "content");
                var encoding = OptionalString(arguments, "encoding") ?? "text";
                byte[] bytes = encoding switch
                {
                    "text" => Encoding.UTF8.GetBytes(content),
                    "base64" => Convert.FromBase64String(content),
                    _ => throw new InvalidOperationException("encoding must be text or base64."),
                };
                if (bytes.Length > 256 * 1_024)
                {
                    throw new InvalidOperationException("The remote upload exceeds 256 KiB.");
                }
                if (OptionalBool(arguments, "create_parent_dirs") ?? true)
                {
                    var parent = RemoteParent(path);
                    if (parent is not null)
                    {
                        var mkdir = await _remoteCommands.ExecuteAsync(
                            connectionId,
                            $"mkdir -p -- {ShellQuote(parent)}",
                            "~",
                            null,
                            cancellationToken).ConfigureAwait(false);
                        if (mkdir.ExitCode != 0)
                        {
                            throw new InvalidOperationException("Failed to create the remote parent directory.");
                        }
                    }
                }
                await using var source = new MemoryStream(bytes, writable: false);
                await _remoteFiles.UploadAsync(
                    connectionId,
                    source,
                    path,
                    OptionalBool(arguments, "overwrite") ?? true,
                    null,
                    cancellationToken).ConfigureAwait(false);
                return JsonSerializer.SerializeToElement(new
                {
                    path,
                    encoding,
                    size_bytes = bytes.Length,
                    uploaded = true,
                }, JsonOptions);
            }
            default:
                throw new InvalidOperationException("Unsupported remote connection tool.");
        }
    }

    private static JsonElement Search(
        WorkspaceFilesystem files,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var path = OptionalString(arguments, "path") ?? ".";
        var query = RequiredString(arguments, "query");
        var limit = OptionalInt(arguments, "limit") ?? 50;
        return string.Equals(OptionalString(arguments, "mode"), "name", StringComparison.Ordinal)
            ? files.SearchEntries(path, query, limit, cancellationToken)
            : files.SearchContent(path, query, limit, cancellationToken);
    }

    private async Task<JsonElement> ExecuteCommandAsync(
        WindowsLocalAgentProjectContext context,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var command = RequiredString(arguments, "command");
        if (command.Length > 1_024 || command.Any(char.IsControl))
        {
            throw new InvalidOperationException("The Local Agent terminal command is invalid.");
        }
        var commandArguments = StringArray(arguments, "arguments", 100);
        var workingDirectory = new WorkspacePathGuard(context.ProjectRoot).ResolveExisting(
            OptionalString(arguments, "working_directory") ?? ".");
        if (!Directory.Exists(workingDirectory))
        {
            throw new InvalidOperationException(
                "The Local Agent terminal working directory is invalid.");
        }

        var result = await _commands.ExecuteAsync(new TerminalCommandRequest(
            command,
            commandArguments,
            workingDirectory,
            context.ProjectRoot,
            context.Workspace.Id,
            WindowsTerminalCommandExecutor.NormalizeTimeout(
                OptionalInt(arguments, "timeout_ms") ?? 0),
            null), cancellationToken).ConfigureAwait(false);
        return JsonSerializer.SerializeToElement(new
        {
            result.Success,
            result.ExitCode,
            result.TimedOut,
            stdout = result.StandardOutput,
            stderr = result.StandardError,
            result.StandardOutputTruncated,
            result.StandardErrorTruncated,
            result.Error,
        }, JsonOptions);
    }

    internal static string RequiredString(JsonElement value, string name)
    {
        var result = OptionalString(value, name);
        if (string.IsNullOrWhiteSpace(result))
        {
            throw new InvalidOperationException($"The Local Agent tool field is invalid: {name}.");
        }
        return result;
    }

    private static string RequiredStringAllowEmpty(JsonElement value, string name)
    {
        if (value.ValueKind != JsonValueKind.Object ||
            !value.TryGetProperty(name, out var property) ||
            property.ValueKind != JsonValueKind.String)
        {
            throw new InvalidOperationException($"The Local Agent tool field is invalid: {name}.");
        }
        return property.GetString() ?? string.Empty;
    }

    internal static string? OptionalString(JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var property) &&
        property.ValueKind == JsonValueKind.String
            ? property.GetString()
            : null;

    private static bool? OptionalBool(JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var property) &&
        property.ValueKind is JsonValueKind.True or JsonValueKind.False
            ? property.GetBoolean()
            : null;

    private static int? OptionalInt(JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var property) &&
        property.TryGetInt32(out var result)
            ? result
            : null;

    private static bool IsDangerousRemoteCommand(string command)
    {
        var normalized = string.Join(' ', command.ToLowerInvariant()
            .Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries));
        return normalized.Contains("rm -rf /", StringComparison.Ordinal) ||
            normalized.Contains("rm -fr /", StringComparison.Ordinal) ||
            normalized.Contains("mkfs", StringComparison.Ordinal) ||
            normalized.Contains("shutdown", StringComparison.Ordinal) ||
            normalized.Contains("poweroff", StringComparison.Ordinal) ||
            normalized.Contains("reboot", StringComparison.Ordinal) ||
            normalized.Contains(":(){:|:&};:", StringComparison.Ordinal) ||
            (normalized.Contains("dd if=", StringComparison.Ordinal) &&
             normalized.Contains(" of=/dev/", StringComparison.Ordinal));
    }

    private static string LimitCharacters(string value, int maximum) =>
        value.Length <= maximum ? value : value[..maximum] + "…";

    private static string? RemoteParent(string path)
    {
        var normalized = path.Trim().Replace('\\', '/');
        var separator = normalized.LastIndexOf('/');
        if (separator < 0) return null;
        if (separator == 0) return "/";
        return normalized[..separator];
    }

    private static string ShellQuote(string value) =>
        "'" + value.Replace("'", "'\\''", StringComparison.Ordinal) + "'";

    internal static IReadOnlyList<string> StringArray(
        JsonElement value,
        string name,
        int maximumCount)
    {
        if (!value.TryGetProperty(name, out var property) || property.ValueKind == JsonValueKind.Null)
        {
            return [];
        }
        if (property.ValueKind != JsonValueKind.Array)
        {
            throw new InvalidOperationException($"The Local Agent tool field is invalid: {name}.");
        }

        var output = property.EnumerateArray().Select(item =>
            item.ValueKind == JsonValueKind.String
                ? item.GetString() ?? string.Empty
                : throw new InvalidOperationException(
                    $"The Local Agent tool field is invalid: {name}.")).ToArray();
        if (output.Length > maximumCount ||
            output.Any(item => item.Length > 8_000 || item.Contains('\0')))
        {
            throw new InvalidOperationException($"The Local Agent tool field is invalid: {name}.");
        }
        return output;
    }
}

internal sealed class BoundedMemoryStream(int maximumBytes) : MemoryStream
{
    private void EnsureCapacityFor(int count)
    {
        if (count < 0 || Length > maximumBytes - count)
        {
            throw new InvalidDataException("The remote file exceeds the selected download limit.");
        }
    }

    public override void Write(byte[] buffer, int offset, int count)
    {
        EnsureCapacityFor(count);
        base.Write(buffer, offset, count);
    }

    public override void Write(ReadOnlySpan<byte> buffer)
    {
        EnsureCapacityFor(buffer.Length);
        base.Write(buffer);
    }

    public override Task WriteAsync(
        byte[] buffer,
        int offset,
        int count,
        CancellationToken cancellationToken)
    {
        EnsureCapacityFor(count);
        return base.WriteAsync(buffer, offset, count, cancellationToken);
    }

    public override ValueTask WriteAsync(
        ReadOnlyMemory<byte> buffer,
        CancellationToken cancellationToken = default)
    {
        EnsureCapacityFor(buffer.Length);
        return base.WriteAsync(buffer, cancellationToken);
    }
}
