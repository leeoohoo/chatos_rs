using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Connector.Relay;

namespace ChatOS.Connector.Workspaces;

public sealed class WorkspaceRelayHandler : IRelayRequestHandler
{
    private static readonly HashSet<string> RequestTypes =
    [
        "workspace_directory_list_request",
        "workspace_directory_create_request",
        "workspace_filesystem_request",
    ];

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    private readonly IConnectorWorkspaceCatalog _workspaces;

    public WorkspaceRelayHandler(IConnectorWorkspaceCatalog workspaces)
    {
        _workspaces = workspaces;
    }

    public bool CanHandle(string requestType) => RequestTypes.Contains(requestType);

    public string ResponseType(string requestType) => requestType switch
    {
        "workspace_directory_list_request" => "workspace_directory_list_response",
        "workspace_directory_create_request" => "workspace_directory_create_response",
        _ => "workspace_filesystem_response",
    };

    public Task<RelayHandlerResult> HandleAsync(
        RelayRequest request,
        CancellationToken cancellationToken)
    {
        var workspace = _workspaces.Find(request.WorkspaceId)
            ?? throw new RelayRequestException(400, "Relay workspace does not belong to this connector.");
        return Task.Run(
            () => RelayHandlerResult.Ok(Process(request, workspace, cancellationToken)),
            cancellationToken);
    }

    private static JsonElement Process(
        RelayRequest request,
        ConnectorWorkspace workspace,
        CancellationToken cancellationToken)
    {
        var filesystem = new WorkspaceFilesystem(workspace);
        if (request.Type is "workspace_directory_list_request")
        {
            var body = Deserialize<WorkspaceDirectoryRequest>(request.Body);
            return filesystem.List(body.Path ?? ".", includeFiles: false);
        }

        if (request.Type is "workspace_directory_create_request")
        {
            var body = Deserialize<WorkspaceDirectoryRequest>(request.Body);
            return filesystem.CreateDirectory(Required(body.Path, "path"));
        }

        var operation = Deserialize<WorkspaceFilesystemRequest>(request.Body);
        return operation.Operation switch
        {
            "list" => filesystem.List(operation.Path ?? ".", includeFiles: true),
            "read" => filesystem.Read(Required(operation.Path, "path")),
            "search_entries" => filesystem.SearchEntries(
                operation.Path ?? ".",
                Required(operation.Query, "query"),
                operation.Limit ?? 200,
                cancellationToken),
            "search_content" => filesystem.SearchContent(
                operation.Path ?? ".",
                Required(operation.Query, "query"),
                operation.Limit ?? 200,
                cancellationToken),
            "create_directory" => filesystem.CreateDirectory(Required(operation.Path, "path")),
            "create_file" => filesystem.Write(
                Required(operation.Path, "path"),
                operation.Content ?? string.Empty,
                createOnly: true),
            "write_file" => filesystem.Write(
                Required(operation.Path, "path"),
                RequiredContent(operation.Content),
                createOnly: false),
            "delete" => filesystem.Delete(
                Required(operation.Path, "path"),
                operation.Recursive ?? false),
            "move" => filesystem.Move(
                Required(operation.SourcePath, "source_path"),
                Required(operation.TargetPath, "target_path"),
                operation.ReplaceExisting ?? false),
            _ => throw new RelayRequestException(
                400,
                $"Unsupported filesystem operation: {operation.Operation}"),
        };
    }

    private static T Deserialize<T>(JsonElement body)
    {
        try
        {
            return body.Deserialize<T>(JsonOptions)
                ?? throw new RelayRequestException(400, "Relay request body is empty.");
        }
        catch (JsonException exception)
        {
            throw new RelayRequestException(400, $"Relay request body is invalid: {exception.Message}");
        }
    }

    private static string Required(string? value, string field) =>
        !string.IsNullOrWhiteSpace(value)
            ? value.Trim()
            : throw new RelayRequestException(400, $"Filesystem request is missing {field}.");

    private static string RequiredContent(string? value) =>
        value ?? throw new RelayRequestException(400, "Filesystem request is missing content.");

    private sealed record WorkspaceDirectoryRequest
    {
        [JsonPropertyName("path")]
        public string? Path { get; init; }
    }

    private sealed record WorkspaceFilesystemRequest
    {
        [JsonPropertyName("operation")]
        public required string Operation { get; init; }

        [JsonPropertyName("path")]
        public string? Path { get; init; }

        [JsonPropertyName("query")]
        public string? Query { get; init; }

        [JsonPropertyName("limit")]
        public int? Limit { get; init; }

        [JsonPropertyName("content")]
        public string? Content { get; init; }

        [JsonPropertyName("recursive")]
        public bool? Recursive { get; init; }

        [JsonPropertyName("source_path")]
        public string? SourcePath { get; init; }

        [JsonPropertyName("target_path")]
        public string? TargetPath { get; init; }

        [JsonPropertyName("replace_existing")]
        public bool? ReplaceExisting { get; init; }
    }
}
