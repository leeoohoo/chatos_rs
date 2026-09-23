using System.Text.Json;
using ChatOS.Connector.Approval;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Terminal;
using ChatOS.Connector.Workspaces;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed class AgentProjectToolExecutor(
    IProjectRegistry projects,
    IConnectorWorkspaceContext workspaces,
    ITerminalCommandExecutor commandExecutor,
    CommandApprovalCoordinator approvals,
    CommandRiskEvaluator riskEvaluator)
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public static IReadOnlyList<AgentToolDefinition> Definitions { get; } =
    [
        Tool("project_list", "列出当前团队项目中的目录和文件。", new
        {
            type = "object",
            properties = new
            {
                path = new { type = "string", description = "相对项目根目录的路径，默认 ." },
                include_files = new { type = "boolean" },
            },
            additionalProperties = false,
        }),
        Tool("project_read", "读取当前团队项目内的一个文本或小型二进制文件。", new
        {
            type = "object",
            properties = new { path = new { type = "string" } },
            required = new[] { "path" },
            additionalProperties = false,
        }),
        Tool("project_search", "按文件名或内容搜索当前团队项目。", new
        {
            type = "object",
            properties = new
            {
                query = new { type = "string" },
                path = new { type = "string", description = "搜索起点，默认 ." },
                mode = new { type = "string", @enum = new[] { "name", "content" } },
                limit = new { type = "integer", minimum = 1, maximum = 100 },
            },
            required = new[] { "query" },
            additionalProperties = false,
        }),
        Tool("project_write", "在当前团队项目中原子创建或更新 UTF-8 文本文件。", new
        {
            type = "object",
            properties = new
            {
                path = new { type = "string" },
                content = new { type = "string" },
                create_only = new { type = "boolean" },
            },
            required = new[] { "path", "content" },
            additionalProperties = false,
        }),
        Tool("terminal_exec", "经 Windows 审批策略执行一个有界的非交互项目命令。", new
        {
            type = "object",
            properties = new
            {
                command = new { type = "string" },
                arguments = new { type = "array", items = new { type = "string" }, maxItems = 100 },
                working_directory = new { type = "string", description = "项目内相对目录，默认 ." },
                timeout_ms = new { type = "integer", minimum = 1_000, maximum = 900_000 },
            },
            required = new[] { "command" },
            additionalProperties = false,
        }),
    ];

    public async Task<string> ExecuteAsync(
        string ownerUserId,
        AgentRoom room,
        string toolName,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var context = await ResolveAsync(ownerUserId, room, cancellationToken).ConfigureAwait(false);
        return toolName switch
        {
            "project_list" => Json(context.Files.List(
                OptionalString(arguments, "path") ?? ".",
                OptionalBool(arguments, "include_files") ?? true)),
            "project_read" => Json(context.Files.Read(RequiredString(arguments, "path"))),
            "project_search" => Json(Search(context.Files, arguments, cancellationToken)),
            "project_write" => Json(context.Files.Write(
                RequiredString(arguments, "path"),
                RequiredString(arguments, "content"),
                OptionalBool(arguments, "create_only") ?? false)),
            "terminal_exec" => await ExecuteCommandAsync(
                ownerUserId, context, arguments, cancellationToken).ConfigureAwait(false),
            _ => throw new AgentTeamException(AgentTeamError.InvalidField,
                $"Unknown project tool: {toolName}"),
        };
    }

    private async Task<ProjectToolsContext> ResolveAsync(
        string ownerUserId,
        AgentRoom room,
        CancellationToken cancellationToken)
    {
        if (room.Kind != AgentConversationKind.ProjectTeam)
        {
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Project tools are available only in project teams.");
        }

        var record = await projects.GetAsync(ownerUserId, room.ProjectId, cancellationToken)
            .ConfigureAwait(false);
        if (record is null || record.Status != LocalProjectStatus.Active)
        {
            throw new AgentTeamException(AgentTeamError.NotFound, "The team project is unavailable.");
        }

        var workspace = workspaces.Find(record.Draft.WorkspaceId)
            ?? throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "The team project workspace is not authorized on this device.");
        var guard = new WorkspacePathGuard(workspace.AbsoluteRoot);
        var relativeRoot = string.IsNullOrEmpty(record.Draft.RelativeRoot)
            ? "."
            : record.Draft.RelativeRoot;
        var projectRoot = guard.ResolveExisting(relativeRoot);
        if (!Directory.Exists(projectRoot))
        {
            throw new AgentTeamException(AgentTeamError.NotFound,
                "The team project directory is unavailable.");
        }

        var projectWorkspace = new ConnectorWorkspace(
            record.Id, record.Draft.Name, projectRoot, workspace.Fingerprint,
            workspace.ProjectConfigTrusted, workspace.ProjectConfigTrustStale);
        return new ProjectToolsContext(record, workspace, projectRoot,
            new WorkspaceFilesystem(projectWorkspace));
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

    private async Task<string> ExecuteCommandAsync(
        string ownerUserId,
        ProjectToolsContext context,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var command = RequiredString(arguments, "command");
        if (command.Length > 1_024 || command.Any(char.IsControl))
        {
            throw AgentTeamValidation.Invalid("command");
        }

        var commandArguments = StringArray(arguments, "arguments", 100);
        var relativeWorkingDirectory = OptionalString(arguments, "working_directory") ?? ".";
        var workingDirectory = new WorkspacePathGuard(context.ProjectRoot)
            .ResolveExisting(relativeWorkingDirectory);
        if (!Directory.Exists(workingDirectory))
        {
            throw new AgentTeamException(AgentTeamError.InvalidField,
                "The command working directory is not a directory.");
        }

        var deviceId = workspaces.DeviceId
            ?? throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "The local Connector is not paired.");
        var requestId = Guid.NewGuid().ToString("N");
        var approvalRequest = new CommandApprovalRequest(
            requestId, ownerUserId, deviceId, context.Workspace.Id, command,
            commandArguments, workingDirectory, "agent-team",
            string.Join('\0', "agent-team", context.Record.Id, workingDirectory,
                command, string.Join('\0', commandArguments)));
        var outcome = await approvals.RequestAsync(approvalRequest,
            riskEvaluator.Evaluate(command, commandArguments), cancellationToken).ConfigureAwait(false);
        if (!outcome.Approved)
        {
            return Json(new
            {
                approved = false,
                error = outcome.Reason,
            });
        }

        var result = await commandExecutor.ExecuteAsync(new TerminalCommandRequest(
            command,
            commandArguments,
            workingDirectory,
            context.ProjectRoot,
            context.Workspace.Id,
            WindowsTerminalCommandExecutor.NormalizeTimeout(OptionalInt(arguments, "timeout_ms") ?? 0),
            null), cancellationToken).ConfigureAwait(false);
        return Json(new
        {
            approved = true,
            result.Success,
            result.ExitCode,
            result.TimedOut,
            stdout = result.StandardOutput,
            stderr = result.StandardError,
            result.StandardOutputTruncated,
            result.StandardErrorTruncated,
            result.Error,
        });
    }

    private static AgentToolDefinition Tool(string name, string description, object schema) =>
        new(name, description, schema);

    private static string Json(object value) => JsonSerializer.Serialize(value, JsonOptions);

    private static string RequiredString(JsonElement value, string name)
    {
        if (value.ValueKind != JsonValueKind.Object ||
            !value.TryGetProperty(name, out var property) ||
            property.ValueKind != JsonValueKind.String ||
            string.IsNullOrWhiteSpace(property.GetString()))
        {
            throw AgentTeamValidation.Invalid(name);
        }

        return property.GetString()!;
    }

    private static string? OptionalString(JsonElement value, string name) =>
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

    private static IReadOnlyList<string> StringArray(
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
            throw AgentTeamValidation.Invalid(name);
        }

        var output = property.EnumerateArray().Select(item =>
            item.ValueKind == JsonValueKind.String
                ? item.GetString() ?? string.Empty
                : throw AgentTeamValidation.Invalid(name)).ToArray();
        if (output.Length > maximumCount || output.Any(item => item.Length > 8_000 || item.Contains('\0')))
        {
            throw AgentTeamValidation.Invalid(name);
        }

        return output;
    }

    private sealed record ProjectToolsContext(
        LocalProjectRecord Record,
        ConnectorWorkspace Workspace,
        string ProjectRoot,
        WorkspaceFilesystem Files);
}
