using System.Text.Json;
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
    WorkspaceFilesystem Files);

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
            new WorkspaceFilesystem(projectWorkspace));
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

    public WindowsLocalAgentProjectToolExecutor(
        ILocalAgentHostClient host,
        WindowsLocalAgentConversationClient conversations,
        IProjectRegistry projects,
        IConnectorWorkspaceContext workspaces,
        ITerminalCommandExecutor commands)
    {
        _contexts = new WindowsLocalAgentProjectContextResolver(
            host, conversations, projects, workspaces);
        _commands = commands;
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

        var context = await _contexts.ResolveAsync(
            ownerUserId, invocation.RunId, cancellationToken).ConfigureAwait(false);
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
