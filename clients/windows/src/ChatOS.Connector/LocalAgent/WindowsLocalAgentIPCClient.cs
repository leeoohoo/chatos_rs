using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentIPCClientFactory : ILocalAgentIPCClientFactory
{
    public ILocalAgentIPCClient Create(string ownerUserId, string pipeName) =>
        new WindowsLocalAgentIPCClient(ownerUserId, pipeName);
}

public sealed class LocalAgentRejectedException(LocalAgentIPCErrorPayload error)
    : Exception(error.Message)
{
    public LocalAgentIPCErrorPayload Error { get; } = error;
}

public sealed class WindowsLocalAgentIPCClient : ILocalAgentIPCClient
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        DictionaryKeyPolicy = null,
        PropertyNameCaseInsensitive = false,
        UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow,
        DefaultIgnoreCondition = JsonIgnoreCondition.Never,
        Converters = { new JsonStringEnumConverter(JsonNamingPolicy.SnakeCaseLower) },
    };

    private readonly string _ownerUserId;
    private readonly ILocalAgentFrameTransport _transport;

    public WindowsLocalAgentIPCClient(string ownerUserId, string pipeName)
        : this(ownerUserId, new NamedPipeLocalAgentTransport(pipeName))
    {
    }

    internal WindowsLocalAgentIPCClient(string ownerUserId, ILocalAgentFrameTransport transport)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(ownerUserId);
        if (!string.Equals(ownerUserId, ownerUserId.Trim(), StringComparison.Ordinal))
        {
            throw new ArgumentException("Local Agent owner user ID cannot contain outer whitespace.", nameof(ownerUserId));
        }
        _ownerUserId = ownerUserId;
        _transport = transport ?? throw new ArgumentNullException(nameof(transport));
    }

    public async Task<LocalAgentResponse> SendAsync(
        LocalAgentCommand command,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(command);
        var requestId = Guid.NewGuid().ToString("D").ToLowerInvariant();
        var request = new RequestEnvelope(
            LocalAgentProtocol.Version,
            requestId,
            _ownerUserId,
            command);
        var requestBytes = JsonSerializer.SerializeToUtf8Bytes(request, JsonOptions);
        var responseBytes = await _transport.ExchangeAsync(requestBytes, cancellationToken)
            .ConfigureAwait(false);

        ReplyEnvelope reply;
        try
        {
            reply = JsonSerializer.Deserialize<ReplyEnvelope>(responseBytes, JsonOptions)
                ?? throw new JsonException("Local Agent reply is empty.");
        }
        catch (JsonException error)
        {
            throw new InvalidDataException("Local Agent Host returned invalid protocol JSON.", error);
        }
        if (reply.ProtocolVersion != LocalAgentProtocol.Version)
        {
            throw new InvalidDataException(
                $"Local Agent protocol version mismatch ({reply.ProtocolVersion}).");
        }
        if (!string.Equals(reply.RequestId, requestId, StringComparison.Ordinal))
        {
            throw new InvalidDataException(
                $"Local Agent response correlation mismatch; expected {requestId}, received {reply.RequestId}.");
        }

        var response = DecodeResponse(reply.Response);
        if (response is LocalAgentErrorResponse rejected)
        {
            throw new LocalAgentRejectedException(rejected.Error);
        }
        return response;
    }

    public async Task<string> AcceptAsync(
        LocalAgentCommand command,
        CancellationToken cancellationToken = default)
    {
        var response = await SendAsync(command, cancellationToken).ConfigureAwait(false);
        return response is LocalAgentAcceptedResponse accepted
            ? accepted.OperationId
            : throw Unexpected("accepted", response.Type);
    }

    public Task<LocalAgentRunCreatedResponse> CreateMainChatTurnAsync(
        LocalAgentCreateMainChatTurn command,
        CancellationToken cancellationToken = default) =>
        SendRunCreatingCommandAsync(LocalAgentCommand.CreateMainChatTurn(command), cancellationToken);

    public Task<LocalAgentRunCreatedResponse> CreateTaskAsync(
        LocalAgentCreateTask command,
        CancellationToken cancellationToken = default) =>
        SendRunCreatingCommandAsync(LocalAgentCommand.CreateTask(command), cancellationToken);

    public Task<LocalAgentRunCreatedResponse> RetryTaskAsync(
        LocalAgentRetryTask command,
        CancellationToken cancellationToken = default) =>
        SendRunCreatingCommandAsync(LocalAgentCommand.RetryTask(command), cancellationToken);

    public async Task<LocalAgentRunSnapshot> GetRunAsync(
        string runId,
        CancellationToken cancellationToken = default)
    {
        var response = await SendAsync(LocalAgentCommand.GetRun(runId), cancellationToken)
            .ConfigureAwait(false);
        return response is LocalAgentRunResponse run
            ? run.Run
            : throw Unexpected("run", response.Type);
    }

    public async Task<LocalAgentTaskSnapshot> GetTaskAsync(
        string taskId,
        CancellationToken cancellationToken = default)
    {
        var response = await SendAsync(LocalAgentCommand.GetTask(taskId), cancellationToken)
            .ConfigureAwait(false);
        return response is LocalAgentTaskResponse task
            ? task.Task
            : throw Unexpected("task", response.Type);
    }

    public async Task<LocalAgentMainChatRunBinding> GetMainChatRunBindingAsync(
        string runId,
        CancellationToken cancellationToken = default)
    {
        var response = await SendAsync(
            LocalAgentCommand.GetMainChatRunBinding(runId),
            cancellationToken).ConfigureAwait(false);
        return response is LocalAgentMainChatRunBindingResponse binding
            ? binding.Binding
            : throw Unexpected("main_chat_run_binding", response.Type);
    }

    public async Task<LocalAgentRunPage> ListRunsAsync(
        string? cursor = null,
        uint limit = 100,
        CancellationToken cancellationToken = default)
    {
        var response = await SendAsync(LocalAgentCommand.ListRuns(cursor, limit), cancellationToken)
            .ConfigureAwait(false);
        return response is LocalAgentRunsResponse page
            ? new LocalAgentRunPage(page.Runs, page.NextCursor)
            : throw Unexpected("runs", response.Type);
    }

    public async Task<LocalAgentTaskPage> ListTasksAsync(
        string? cursor = null,
        uint limit = 100,
        CancellationToken cancellationToken = default)
    {
        var response = await SendAsync(LocalAgentCommand.ListTasks(cursor, limit), cancellationToken)
            .ConfigureAwait(false);
        return response is LocalAgentTasksResponse page
            ? new LocalAgentTaskPage(page.Tasks, page.NextCursor)
            : throw Unexpected("tasks", response.Type);
    }

    public async Task<LocalAgentEventPage> SubscribeRunEventsAsync(
        ulong afterSequence,
        uint limit = 200,
        CancellationToken cancellationToken = default)
    {
        var response = await SendAsync(
            LocalAgentCommand.SubscribeRunEvents(afterSequence, limit),
            cancellationToken).ConfigureAwait(false);
        return response is LocalAgentEventsResponse page
            ? new LocalAgentEventPage(page.Events, page.NextSequence, page.HasMore)
            : throw Unexpected("events", response.Type);
    }

    public async Task<ulong> GetUIEventCursorAsync(CancellationToken cancellationToken = default)
    {
        var response = await SendAsync(LocalAgentCommand.GetUIEventCursor(), cancellationToken)
            .ConfigureAwait(false);
        return response is LocalAgentUIEventCursorResponse cursor
            ? cursor.EventSequence
            : throw Unexpected("ui_event_cursor", response.Type);
    }

    public async Task<ulong> AcknowledgeUIEventsAsync(
        ulong throughSequence,
        CancellationToken cancellationToken = default)
    {
        var response = await SendAsync(
            LocalAgentCommand.AcknowledgeUIEvents(throughSequence),
            cancellationToken).ConfigureAwait(false);
        return response is LocalAgentUIEventCursorResponse cursor
            ? cursor.EventSequence
            : throw Unexpected("ui_event_cursor", response.Type);
    }

    private async Task<LocalAgentRunCreatedResponse> SendRunCreatingCommandAsync(
        LocalAgentCommand command,
        CancellationToken cancellationToken)
    {
        var response = await SendAsync(command, cancellationToken).ConfigureAwait(false);
        return response is LocalAgentRunCreatedResponse created
            ? created
            : throw Unexpected("run_created", response.Type);
    }

    private static LocalAgentResponse DecodeResponse(JsonElement value)
    {
        if (value.ValueKind != JsonValueKind.Object ||
            !value.TryGetProperty("type", out var typeValue) ||
            typeValue.ValueKind != JsonValueKind.String)
        {
            throw new InvalidDataException("Local Agent reply has no response type.");
        }
        var type = typeValue.GetString()!;
        var hasPayload = value.TryGetProperty("payload", out var payload);
        foreach (var property in value.EnumerateObject())
        {
            if (property.Name is not ("type" or "payload"))
            {
                throw new InvalidDataException(
                    $"Local Agent '{type}' response contains unknown field '{property.Name}'.");
            }
        }
        try
        {
            return type switch
            {
                "accepted" => new LocalAgentAcceptedResponse(
                    RequirePayload<AcceptedPayload>(hasPayload, payload).OperationId),
                "run_created" => RunCreated(RequirePayload<RunCreatedPayload>(hasPayload, payload)),
                "run" => new LocalAgentRunResponse(RequirePayload<LocalAgentRunSnapshot>(hasPayload, payload)),
                "task" => new LocalAgentTaskResponse(RequirePayload<LocalAgentTaskSnapshot>(hasPayload, payload)),
                "main_chat_run_binding" => new LocalAgentMainChatRunBindingResponse(
                    RequirePayload<LocalAgentMainChatRunBinding>(hasPayload, payload)),
                "runs" => Runs(RequirePayload<RunsPayload>(hasPayload, payload)),
                "tasks" => Tasks(RequirePayload<TasksPayload>(hasPayload, payload)),
                "events" => Events(RequirePayload<EventsPayload>(hasPayload, payload)),
                "ui_event_cursor" => new LocalAgentUIEventCursorResponse(
                    RequirePayload<UIEventCursorPayload>(hasPayload, payload).EventSeq),
                "storage_profile" => new LocalAgentStorageProfileResponse(
                    RequirePayload<LocalAgentStorageProfile>(hasPayload, payload)),
                "postgres_connection_test" => new LocalAgentPostgresConnectionTestResponse(
                    RequirePayload<LocalAgentPostgresConnectionTest>(hasPayload, payload)),
                "data_transfer" => new LocalAgentDataTransferResponse(
                    RequirePayload<LocalAgentDataTransfer>(hasPayload, payload)),
                "success" when !hasPayload => new LocalAgentSuccessResponse(),
                "error" => new LocalAgentErrorResponse(
                    RequirePayload<LocalAgentIPCErrorPayload>(hasPayload, payload)),
                _ => throw new InvalidDataException($"Unknown Local Agent response type '{type}'."),
            };
        }
        catch (JsonException error)
        {
            throw new InvalidDataException($"Local Agent '{type}' response payload is invalid.", error);
        }
    }

    private static T RequirePayload<T>(bool hasPayload, JsonElement payload)
    {
        if (!hasPayload)
        {
            throw new InvalidDataException("Local Agent response payload is missing.");
        }
        return payload.Deserialize<T>(JsonOptions)
            ?? throw new InvalidDataException("Local Agent response payload is empty.");
    }

    private static LocalAgentRunsResponse Runs(RunsPayload payload) =>
        new(payload.Runs, payload.NextCursor);

    private static LocalAgentRunCreatedResponse RunCreated(RunCreatedPayload payload) =>
        new(payload.OperationId, payload.Run);

    private static LocalAgentTasksResponse Tasks(TasksPayload payload) =>
        new(payload.Tasks, payload.NextCursor);

    private static LocalAgentEventsResponse Events(EventsPayload payload) =>
        new(payload.Events, payload.NextSeq, payload.HasMore);

    private static InvalidDataException Unexpected(string expected, string actual) =>
        new($"Local Agent response type mismatch; expected {expected}, received {actual}.");

    private sealed record RequestEnvelope(
        uint ProtocolVersion,
        string RequestId,
        string OwnerUserId,
        LocalAgentCommand Command);

    private sealed record ReplyEnvelope(
        uint ProtocolVersion,
        string RequestId,
        JsonElement Response);

    private sealed record AcceptedPayload(string OperationId);
    private sealed record RunCreatedPayload(string OperationId, LocalAgentRunSnapshot Run);
    private sealed record RunsPayload(IReadOnlyList<LocalAgentRunSnapshot> Runs, string? NextCursor);
    private sealed record TasksPayload(IReadOnlyList<LocalAgentTaskSnapshot> Tasks, string? NextCursor);
    private sealed record UIEventCursorPayload(ulong EventSeq);
    private sealed record EventsPayload(
        IReadOnlyList<LocalAgentUIEvent> Events,
        ulong NextSeq,
        bool HasMore);
}
