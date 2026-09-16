using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Terminal;
using ChatOS.Connector.Workspaces;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Remote;

public sealed class RemoteTerminalRelayHandler : IRelayRequestHandler, IRelayOneWayHandler
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    private readonly RemoteTerminalSessionManager _sessions;
    private readonly ConnectorOutboundEventHub _events;
    private readonly IConnectorWorkspaceCatalog _workspaces;

    public RemoteTerminalRelayHandler(
        RemoteTerminalSessionManager sessions,
        ConnectorOutboundEventHub events,
        IConnectorWorkspaceCatalog workspaces)
    {
        _sessions = sessions;
        _events = events;
        _workspaces = workspaces;
    }

    bool IRelayRequestHandler.CanHandle(string requestType) =>
        requestType == "remote_terminal_session_create_request";

    bool IRelayOneWayHandler.CanHandle(string requestType) => requestType is
        "remote_terminal_input" or
        "remote_terminal_resize" or
        "remote_terminal_snapshot_request" or
        "remote_terminal_close";

    public string ResponseType(string requestType) => "terminal_session_create_response";

    async Task<RelayHandlerResult> IRelayRequestHandler.HandleAsync(
        RelayRequest request,
        CancellationToken cancellationToken)
    {
        if (_workspaces.Find(request.WorkspaceId) is null)
        {
            throw new RelayRequestException(
                400,
                "Remote terminal workspace is not registered locally.");
        }
        var body = Deserialize<RemoteTerminalCreateBody>(request.Body);
        var sessionId = Required(body.TerminalSessionId, "terminal_session_id");
        var connectionId = Required(body.Connection?.Id, "connection.id");
        var identity = new RemoteTerminalSessionIdentity(
            sessionId,
            Required(request.WorkspaceId, "workspace_id"),
            connectionId);

        try
        {
            var session = await _sessions.EnsureSessionAsync(
                identity,
                TerminalSize.Normalize(body.Columns ?? 80, body.Rows ?? 24),
                Clean(body.VerificationCode),
                cancellationToken).ConfigureAwait(false);
            var snapshot = session.SnapshotState(500);
            return RelayHandlerResult.Ok(JsonSerializer.SerializeToElement(new
            {
                terminal_session_id = sessionId,
                snapshot = snapshot.Data,
                base_sequence = snapshot.BaseSequence,
                sequence = snapshot.Sequence,
                truncated = snapshot.Truncated,
                protocol_version = 2,
                busy = false,
            }, JsonOptions));
        }
        catch (RemoteVerificationRequiredException challenge)
        {
            return new RelayHandlerResult(409, JsonSerializer.SerializeToElement(new
            {
                code = "second_factor_required",
                error = challenge.Prompt,
                prompt = challenge.Prompt,
                recoverable = true,
            }, JsonOptions));
        }
    }

    async Task IRelayOneWayHandler.HandleAsync(
        RelayRequest request,
        CancellationToken cancellationToken)
    {
        var body = Deserialize<RemoteTerminalControlBody>(request.Body);
        var sessionId = Required(body.TerminalSessionId, "terminal_session_id");
        var session = await _sessions.GetAsync(sessionId).ConfigureAwait(false);
        if (session is null ||
            !string.Equals(session.Identity.WorkspaceId, request.WorkspaceId, StringComparison.Ordinal))
        {
            PublishError(
                sessionId,
                "Remote terminal session was not found for this workspace.",
                "remote_terminal_not_found",
                recoverable: true);
            return;
        }

        try
        {
            switch (request.Type)
            {
                case "remote_terminal_input":
                    await session.WriteAsync(body.Data ?? string.Empty, cancellationToken)
                        .ConfigureAwait(false);
                    break;
                case "remote_terminal_resize":
                    await session.ResizeAsync(
                        TerminalSize.Normalize(body.Columns ?? 80, body.Rows ?? 24),
                        cancellationToken).ConfigureAwait(false);
                    break;
                case "remote_terminal_snapshot_request":
                    var snapshot = session.SnapshotState(body.Lines ?? 500);
                    _events.Publish(new TerminalEvent(
                        TerminalEventKind.Snapshot,
                        sessionId,
                        Data: snapshot.Data,
                        Sequence: snapshot.Sequence,
                        BaseSequence: snapshot.BaseSequence,
                        Truncated: snapshot.Truncated,
                        Remote: true));
                    break;
                case "remote_terminal_close":
                    await _sessions.CloseAsync(sessionId, cancellationToken).ConfigureAwait(false);
                    break;
            }
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            PublishError(
                sessionId,
                exception.Message,
                "remote_terminal_control_failed",
                recoverable: request.Type != "remote_terminal_close");
        }
    }

    private void PublishError(string sessionId, string message, string code, bool recoverable) =>
        _events.Publish(new TerminalEvent(
            TerminalEventKind.Error,
            sessionId,
            Data: message,
            Remote: true,
            ErrorCode: code,
            Recoverable: recoverable));

    private static T Deserialize<T>(JsonElement value)
    {
        try
        {
            return value.Deserialize<T>(JsonOptions)
                ?? throw new RelayRequestException(400, "Remote terminal Relay body is empty.");
        }
        catch (JsonException exception)
        {
            throw new RelayRequestException(
                400,
                $"Remote terminal Relay body is invalid: {exception.Message}");
        }
    }

    private static string Required(string? value, string field) =>
        !string.IsNullOrWhiteSpace(value)
            ? value.Trim()
            : throw new RelayRequestException(
                400,
                $"Remote terminal Relay is missing {field}.");

    private static string? Clean(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();

    private sealed record RemoteTerminalCreateBody
    {
        [JsonPropertyName("terminal_session_id")]
        public string? TerminalSessionId { get; init; }

        [JsonPropertyName("connection")]
        public RemoteConnectionReference? Connection { get; init; }

        [JsonPropertyName("verification_code")]
        public string? VerificationCode { get; init; }

        [JsonPropertyName("cols")]
        public int? Columns { get; init; }

        [JsonPropertyName("rows")]
        public int? Rows { get; init; }
    }

    private sealed record RemoteConnectionReference
    {
        [JsonPropertyName("id")]
        public string? Id { get; init; }
    }

    private sealed record RemoteTerminalControlBody
    {
        [JsonPropertyName("terminal_session_id")]
        public string? TerminalSessionId { get; init; }

        [JsonPropertyName("data")]
        public string? Data { get; init; }

        [JsonPropertyName("cols")]
        public int? Columns { get; init; }

        [JsonPropertyName("rows")]
        public int? Rows { get; init; }

        [JsonPropertyName("lines")]
        public int? Lines { get; init; }
    }
}
