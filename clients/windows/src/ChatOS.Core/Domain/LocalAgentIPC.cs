using System.Text.Json;
using System.Text.Json.Serialization;

namespace ChatOS.Core.Domain;

public static class LocalAgentProtocol
{
    public const uint Version = 6;
    public const int MaximumFrameBytes = 8 * 1024 * 1024;
}

public sealed record LocalAgentFrozenSnapshot(
    string SnapshotId,
    string Revision,
    string Digest,
    JsonElement Payload);

public sealed record LocalAgentAttachmentReference(
    string AttachmentId,
    string MediaType,
    string PayloadReference,
    string PayloadDigest,
    ulong ByteSize);

public sealed record LocalAgentCreateMainChatTurn(
    string ThreadId,
    string TurnId,
    string MessageId,
    string? ProjectId,
    string ModelConfigId,
    LocalAgentFrozenSnapshot PromptSnapshot,
    LocalAgentFrozenSnapshot CapabilitySnapshot,
    LocalAgentFrozenSnapshot? ProjectSnapshot,
    string? Content,
    IReadOnlyList<LocalAgentAttachmentReference> Attachments);

public sealed record LocalAgentCreateTask(
    string TaskId,
    string SourceThreadId,
    string SourceTurnId,
    string ProjectId,
    string Objective,
    IReadOnlyList<string> AcceptanceCriteria,
    string ModelConfigId,
    LocalAgentFrozenSnapshot PromptSnapshot,
    LocalAgentFrozenSnapshot ProjectSnapshot,
    LocalAgentFrozenSnapshot CapabilitySnapshot);

public sealed record LocalAgentUserAnswer(
    string? Text,
    IReadOnlyList<string> SelectedOptionIds,
    IReadOnlyList<LocalAgentAttachmentReference> Attachments);

public enum LocalAgentToolApprovalDecision
{
    Approve,
    Reject,
}

/// <summary>
/// Exact representation of Rust's internally tagged LocalAgentCommand enum.
/// The factories prevent callers from accidentally creating a type/payload mismatch.
/// </summary>
public sealed record LocalAgentCommand
{
    public string Type { get; }

    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public object? Payload { get; }

    private LocalAgentCommand(string type, object? payload)
    {
        Type = type;
        Payload = payload;
    }

    public static LocalAgentCommand CreateMainChatTurn(LocalAgentCreateMainChatTurn value) =>
        new("create_main_chat_turn", value);

    public static LocalAgentCommand CreateTask(LocalAgentCreateTask value) => new("create_task", value);
    public static LocalAgentCommand PauseRun(string runId) => RunCommand("pause_run", runId);
    public static LocalAgentCommand ResumeRun(string runId) => RunCommand("resume_run", runId);
    public static LocalAgentCommand CancelRun(string runId) => RunCommand("cancel_run", runId);
    public static LocalAgentCommand GetRun(string runId) => RunCommand("get_run", runId);

    public static LocalAgentCommand AnswerUserQuestion(
        string runId,
        string interactionId,
        LocalAgentUserAnswer answer) =>
        new("answer_user_question", new AnswerPayload(runId, interactionId, answer));

    public static LocalAgentCommand DecideToolApproval(
        string invocationId,
        LocalAgentToolApprovalDecision decision,
        string? reason = null) =>
        new("decide_tool_approval", new ApprovalPayload(invocationId, decision, reason));

    public static LocalAgentCommand ListRuns(string? cursor = null, uint limit = 100) =>
        new("list_runs", new ListPayload(cursor, limit));

    public static LocalAgentCommand SubscribeRunEvents(ulong afterSequence, uint limit = 200) =>
        new("subscribe_run_events", new EventsPayload(afterSequence, limit));

    public static LocalAgentCommand GetStorageProfile() => new("get_storage_profile", null);

    public static LocalAgentCommand TestPostgresConnection(string connectionSecretReference) =>
        new("test_postgres_connection", new PostgresTestPayload(connectionSecretReference));

    public static LocalAgentCommand ApplyStorageProfile(
        LocalAgentStorageProfileSelection profile,
        bool confirmNoActiveRuns) =>
        new("apply_storage_profile", new ApplyStoragePayload(profile, confirmNoActiveRuns));

    public static LocalAgentCommand ExportClientData(
        string destinationReference,
        bool includeLargePayloadReferences) =>
        new("export_client_data", new ExportPayload(destinationReference, includeLargePayloadReferences));

    public static LocalAgentCommand ImportClientData(
        string sourceReference,
        string expectedArchiveDigest,
        bool confirmNoActiveRuns) =>
        new("import_client_data", new ImportPayload(
            sourceReference,
            expectedArchiveDigest,
            confirmNoActiveRuns));

    public static LocalAgentCommand InstallProjectPluginCapability(
        string projectId,
        string pluginId,
        string releaseId,
        JsonElement capabilityRecord) =>
        new("install_project_plugin_capability", new InstallPluginCapabilityPayload(
            projectId,
            pluginId,
            releaseId,
            capabilityRecord));

    public static LocalAgentCommand RemoveProjectPluginCapability(
        string projectId,
        string pluginId,
        string releaseId) =>
        new("remove_project_plugin_capability", new RemovePluginCapabilityPayload(
            projectId,
            pluginId,
            releaseId));

    private static LocalAgentCommand RunCommand(string type, string runId) =>
        new(type, new RunPayload(runId));

    private sealed record RunPayload(string RunId);
    private sealed record ListPayload(string? Cursor, uint Limit);
    private sealed record EventsPayload(ulong AfterSeq, uint Limit);
    private sealed record AnswerPayload(string RunId, string InteractionId, LocalAgentUserAnswer Answer);
    private sealed record ApprovalPayload(
        string InvocationId,
        LocalAgentToolApprovalDecision Decision,
        string? Reason);
    private sealed record PostgresTestPayload(string ConnectionSecretReference);
    private sealed record ApplyStoragePayload(
        LocalAgentStorageProfileSelection Profile,
        bool ConfirmNoActiveRuns);
    private sealed record ExportPayload(
        string DestinationReference,
        bool IncludeLargePayloadReferences);
    private sealed record ImportPayload(
        string SourceReference,
        string ExpectedArchiveDigest,
        bool ConfirmNoActiveRuns);
    private sealed record InstallPluginCapabilityPayload(
        string ProjectId,
        string PluginId,
        string ReleaseId,
        JsonElement CapabilityRecord);
    private sealed record RemovePluginCapabilityPayload(
        string ProjectId,
        string PluginId,
        string ReleaseId);
}

public sealed record LocalAgentStorageProfileSelection
{
    public string Backend { get; }

    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? DatabaseReference { get; }

    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? EncryptionSecretReference { get; }

    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? ConnectionSecretReference { get; }

    private LocalAgentStorageProfileSelection(
        string backend,
        string? databaseReference,
        string? encryptionSecretReference,
        string? connectionSecretReference)
    {
        Backend = backend;
        DatabaseReference = databaseReference;
        EncryptionSecretReference = encryptionSecretReference;
        ConnectionSecretReference = connectionSecretReference;
    }

    public static LocalAgentStorageProfileSelection Sqlite(
        string databaseReference,
        string encryptionSecretReference) =>
        new("sqlite", databaseReference, encryptionSecretReference, null);

    public static LocalAgentStorageProfileSelection Postgres(string connectionSecretReference) =>
        new("postgres", null, null, connectionSecretReference);
}

public enum LocalAgentRunStatus
{
    Queued,
    ModelReady,
    ModelRunning,
    WaitingToolResult,
    ContinuationReady,
    RetryScheduled,
    Paused,
    NeedsReview,
    Succeeded,
    Failed,
    Cancelled,
}

public sealed record LocalAgentRunSnapshot(
    string RunId,
    string ProfileKey,
    string OwnerUserId,
    string OwnerEntityType,
    string OwnerEntityId,
    string? ProjectId,
    LocalAgentRunStatus Status,
    ulong Version,
    ulong StepSeq,
    uint Iteration,
    uint RetryCount,
    string ModelConfigId,
    ulong ModelConfigRevision,
    JsonElement ModelRuntimeSnapshot,
    string ContextStrategy,
    string PromptRevision,
    string CapabilitySnapshotRef,
    string? PendingBatchId,
    JsonElement? PendingInteraction,
    JsonElement? TerminalOutcome,
    DateTimeOffset? DeadlineAt,
    DateTimeOffset CreatedAt,
    DateTimeOffset UpdatedAt);

public sealed record LocalAgentTaggedValue(string Type, JsonElement? Payload);

public sealed record LocalAgentUIEvent(
    ulong EventSeq,
    DateTimeOffset EmittedAt,
    LocalAgentTaggedValue Event);

public sealed record LocalAgentStorageProfile(
    string Backend,
    string Health,
    string? SqliteDatabaseReference,
    string? PostgresConnectionSecretReference,
    uint SchemaVersion,
    string? LastErrorCode);

public sealed record LocalAgentPostgresConnectionTest(
    string ServerVersion,
    bool TlsActive,
    bool AuthenticationOk,
    bool TransactionOk,
    bool MigrationPermissionOk);

public sealed record LocalAgentDataTransfer(
    string ArchiveReference,
    string ArchiveDigest,
    ulong RecordCount);

public sealed record LocalAgentIPCErrorPayload(
    string Code,
    string Message,
    bool Retryable);

public abstract record LocalAgentResponse(string Type);
public sealed record LocalAgentAcceptedResponse(string OperationId) : LocalAgentResponse("accepted");
public sealed record LocalAgentRunResponse(LocalAgentRunSnapshot Run) : LocalAgentResponse("run");
public sealed record LocalAgentRunsResponse(
    IReadOnlyList<LocalAgentRunSnapshot> Runs,
    string? NextCursor) : LocalAgentResponse("runs");
public sealed record LocalAgentEventsResponse(
    IReadOnlyList<LocalAgentUIEvent> Events,
    ulong NextSequence,
    bool HasMore) : LocalAgentResponse("events");
public sealed record LocalAgentStorageProfileResponse(
    LocalAgentStorageProfile Profile) : LocalAgentResponse("storage_profile");
public sealed record LocalAgentPostgresConnectionTestResponse(
    LocalAgentPostgresConnectionTest Result) : LocalAgentResponse("postgres_connection_test");
public sealed record LocalAgentDataTransferResponse(
    LocalAgentDataTransfer Result) : LocalAgentResponse("data_transfer");
public sealed record LocalAgentSuccessResponse() : LocalAgentResponse("success");
public sealed record LocalAgentErrorResponse(
    LocalAgentIPCErrorPayload Error) : LocalAgentResponse("error");

public sealed record LocalAgentRunPage(
    IReadOnlyList<LocalAgentRunSnapshot> Runs,
    string? NextCursor);

public sealed record LocalAgentEventPage(
    IReadOnlyList<LocalAgentUIEvent> Events,
    ulong NextSequence,
    bool HasMore);
