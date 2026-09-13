using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentMainChatService : ILocalAgentMainChatService, IDisposable
{
    private readonly IWindowsLocalAgentProjectionStore _store;
    private readonly IWindowsLocalAgentAccountSession _accountSession;
    private readonly IConversationRuntimeSettingsService _runtimeSettings;
    private readonly ILocalAgentContactRuntimeContextService _contactContexts;
    private readonly IProjectRegistry _projects;
    private readonly WindowsLocalAgentMainChatSnapshotFactory _snapshots;

    public WindowsLocalAgentMainChatService(
        IWindowsLocalAgentProjectionStore store,
        IWindowsLocalAgentAccountSession accountSession,
        IConversationRuntimeSettingsService runtimeSettings,
        ILocalAgentContactRuntimeContextService contactContexts,
        IProjectRegistry projects,
        WindowsLocalAgentMainChatSnapshotFactory snapshots)
    {
        _store = store;
        _accountSession = accountSession;
        _runtimeSettings = runtimeSettings;
        _contactContexts = contactContexts;
        _projects = projects;
        _snapshots = snapshots;
        _store.Changed += OnProjectionChanged;
        _store.Cleared += OnProjectionCleared;
    }

    public event EventHandler? ProjectionChanged;
    public event EventHandler? ProjectionCleared;

    public async Task<LocalAgentConversationSnapshot> GetConversationAsync(
        string threadId,
        CancellationToken cancellationToken = default)
    {
        RequireIdentity(threadId, nameof(threadId));
        var projection = await RequireProjectionAsync(cancellationToken).ConfigureAwait(false);
        var turns = projection.Runs.Values
            .Where(value => value.Run.ProfileKey == "main_chat"
                && value.Run.OwnerEntityType == "conversation"
                && string.Equals(value.Run.OwnerEntityId, threadId, StringComparison.Ordinal))
            .Select(value => ValidateAndProject(
                projection.AccountId,
                threadId,
                value,
                projection.Tasks.Values.Where(task =>
                        string.Equals(task.SourceThreadId, threadId, StringComparison.Ordinal)
                        && string.Equals(
                            task.SourceTurnId,
                            value.MainChatBinding?.TurnId,
                            StringComparison.Ordinal))
                    .OrderBy(task => task.CreatedAt)
                    .ThenBy(task => task.TaskId, StringComparer.Ordinal)
                    .ToArray()))
            .OrderBy(value => value.Binding.UserMessage.CreatedAt)
            .ThenBy(value => value.Binding.TurnId, StringComparer.Ordinal)
            .ToArray();
        if (turns.Select(value => value.Binding.TurnId).Distinct(StringComparer.Ordinal).Count()
            != turns.Length)
        {
            throw new InvalidDataException("The Local Agent conversation contains duplicate turn identities.");
        }
        return new LocalAgentConversationSnapshot(projection.AccountId, threadId, turns);
    }

    public async Task<LocalAgentRunCreatedResponse> CreateTurnAsync(
        LocalAgentCreateConversationTurn command,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(command);
        ValidateScope(command.Scope);
        RequireIdentity(command.TurnId, nameof(command.TurnId));
        RequireIdentity(command.MessageId, nameof(command.MessageId));
        var content = command.Content?.Trim();
        if (string.IsNullOrEmpty(content) && command.Attachments.Count == 0)
        {
            throw new ArgumentException("A Main Chat turn requires content or attachments.", nameof(command));
        }
        var projection = await RequireProjectionAsync(cancellationToken).ConfigureAwait(false);
        if (!string.Equals(projection.AccountId, command.Scope.AccountId, StringComparison.Ordinal))
        {
            throw new InvalidOperationException("The Main Chat scope belongs to another Local Agent account.");
        }
        if (projection.Runs.Values.Any(value =>
                value.Run.ProfileKey == "main_chat"
                && value.Run.OwnerEntityType == "conversation"
                && string.Equals(value.Run.OwnerEntityId, command.Scope.ThreadId, StringComparison.Ordinal)
                && !IsTerminal(value.Run.Status)))
        {
            throw new InvalidOperationException(
                "The Main Chat conversation already has an active Local Agent run.");
        }
        if (projection.Runs.Values.Any(value =>
                string.Equals(value.MainChatBinding?.TurnId, command.TurnId, StringComparison.Ordinal)
                || string.Equals(value.MainChatBinding?.MessageId, command.MessageId, StringComparison.Ordinal)))
        {
            throw new InvalidOperationException("The Main Chat turn or message identity already exists.");
        }

        var settingsTask = _runtimeSettings.FetchAsync(command.Scope.ThreadId, cancellationToken);
        var contactTask = OptionalContactAsync(command.Scope.ContactAgentId, cancellationToken);
        var projectTask = command.Scope.ProjectId is { } projectId
            ? _projects.GetAsync(command.Scope.AccountId, projectId, cancellationToken)
            : Task.FromResult<LocalProjectRecord?>(null);
        await Task.WhenAll(settingsTask, contactTask, projectTask).ConfigureAwait(false);
        var settings = await settingsTask.ConfigureAwait(false);
        var modelConfigId = settings.SelectedModelId;
        RequireIdentity(modelConfigId, "SelectedModelId");
        var contact = await contactTask.ConfigureAwait(false);
        if (contact is not null
            && !string.Equals(contact.AgentId, command.Scope.ContactAgentId, StringComparison.Ordinal))
        {
            throw new InvalidDataException("The Main Chat contact runtime identity changed.");
        }
        var project = await projectTask.ConfigureAwait(false);
        if (command.Scope.ProjectId is not null
            && (project is null
                || project.Status != LocalProjectStatus.Active
                || !string.Equals(project.Id, command.Scope.ProjectId, StringComparison.Ordinal)
                || !string.Equals(project.OwnerUserId, command.Scope.AccountId, StringComparison.Ordinal)))
        {
            throw new InvalidDataException("The Main Chat project identity changed.");
        }

        var frozen = _snapshots.Make(contact, project);
        var references = await _accountSession.StageAttachmentsAsync(
            command.Scope.AccountId,
            command.Attachments,
            cancellationToken).ConfigureAwait(false);
        LocalAgentRunCreatedResponse created;
        ILocalAgentIPCClient client;
        try
        {
            client = await _accountSession.GetClientAsync(
                command.Scope.AccountId,
                cancellationToken).ConfigureAwait(false);
            created = await client.CreateMainChatTurnAsync(
                new LocalAgentCreateMainChatTurn(
                    command.Scope.ThreadId,
                    command.TurnId,
                    command.MessageId,
                    command.Scope.ProjectId,
                    modelConfigId!,
                    frozen.Prompt,
                    frozen.Capabilities,
                    frozen.Project,
                    content,
                    references),
                cancellationToken).ConfigureAwait(false);
        }
        catch
        {
            await _accountSession.DiscardStagedAttachmentsAsync(
                command.Scope.AccountId,
                references).ConfigureAwait(false);
            throw;
        }
        ValidateCreated(command.Scope, command.TurnId, modelConfigId!, created);
        var detail = await WindowsLocalAgentStartupRecovery.CompleteDetailAsync(
            client,
            created.Run.RunId,
            cancellationToken).ConfigureAwait(false);
        WindowsLocalAgentStartupRecovery.ValidateRunDetail(created.Run, detail.Run);
        var binding = await client.GetMainChatRunBindingAsync(
            created.Run.RunId,
            cancellationToken).ConfigureAwait(false);
        WindowsLocalAgentStartupRecovery.ValidateBinding(created.Run, binding);
        if (!string.Equals(binding.TurnId, command.TurnId, StringComparison.Ordinal)
            || !string.Equals(binding.MessageId, command.MessageId, StringComparison.Ordinal))
        {
            throw new InvalidDataException("The created Main Chat binding changed its turn identity.");
        }
        await _store.UpsertCreatedRunAsync(
            command.Scope.AccountId,
            new WindowsLocalAgentRecoveredRun(
                detail.Run,
                detail,
                binding,
                detail.SnapshotEventSequence),
            cancellationToken).ConfigureAwait(false);
        return created;
    }

    public async Task CancelTurnAsync(
        string threadId,
        string turnId,
        string runId,
        ulong expectedVersion,
        CancellationToken cancellationToken = default)
    {
        RequireIdentity(threadId, nameof(threadId));
        RequireIdentity(turnId, nameof(turnId));
        RequireIdentity(runId, nameof(runId));
        var projection = await RequireProjectionAsync(cancellationToken).ConfigureAwait(false);
        if (!projection.Runs.TryGetValue(runId, out var recovered))
        {
            throw new KeyNotFoundException("The Local Agent Main Chat run was not found.");
        }
        var turn = ValidateAndProject(projection.AccountId, threadId, recovered, []);
        if (!string.Equals(turn.Binding.TurnId, turnId, StringComparison.Ordinal)
            || turn.Run.Version != expectedVersion)
        {
            throw new InvalidOperationException("The Main Chat run changed before cancellation.");
        }
        if (IsTerminal(turn.Run.Status))
        {
            throw new InvalidOperationException("A terminal Main Chat run cannot be cancelled.");
        }
        var client = await _accountSession.GetClientAsync(projection.AccountId, cancellationToken)
            .ConfigureAwait(false);
        _ = await client.AcceptAsync(
            LocalAgentCommand.CancelRun(runId, expectedVersion),
            cancellationToken).ConfigureAwait(false);
    }

    public void Dispose()
    {
        _store.Changed -= OnProjectionChanged;
        _store.Cleared -= OnProjectionCleared;
    }

    private async Task<WindowsLocalAgentProjectionSnapshot> RequireProjectionAsync(
        CancellationToken cancellationToken) =>
        await _store.GetAsync(cancellationToken).ConfigureAwait(false)
        ?? throw new InvalidOperationException("The Local Agent account projection is not available.");

    private async Task<LocalAgentContactRuntimeContext?> OptionalContactAsync(
        string? agentId,
        CancellationToken cancellationToken) => agentId is null
        ? null
        : await _contactContexts.FetchAsync(agentId, cancellationToken).ConfigureAwait(false);

    private static LocalAgentMainChatTurn ValidateAndProject(
        string accountId,
        string threadId,
        WindowsLocalAgentRecoveredRun recovered,
        IReadOnlyList<LocalAgentTaskSnapshot> tasks)
    {
        var run = recovered.Run;
        var detail = recovered.Detail
            ?? throw new InvalidDataException("The Main Chat run has no authoritative detail.");
        var binding = recovered.MainChatBinding
            ?? throw new InvalidDataException("The Main Chat run has no user message binding.");
        if (!string.Equals(run.OwnerUserId, accountId, StringComparison.Ordinal)
            || !string.Equals(run.ProfileKey, "main_chat", StringComparison.Ordinal)
            || !string.Equals(run.OwnerEntityType, "conversation", StringComparison.Ordinal)
            || !string.Equals(run.OwnerEntityId, threadId, StringComparison.Ordinal)
            || !string.Equals(detail.Run.RunId, run.RunId, StringComparison.Ordinal)
            || detail.Run.Version != run.Version
            || !string.Equals(binding.RunId, run.RunId, StringComparison.Ordinal)
            || !string.Equals(binding.ThreadId, threadId, StringComparison.Ordinal)
            || !string.Equals(binding.UserMessage.RunId, run.RunId, StringComparison.Ordinal)
            || !string.Equals(binding.UserMessage.ThreadId, threadId, StringComparison.Ordinal)
            || !string.Equals(binding.UserMessage.TurnId, binding.TurnId, StringComparison.Ordinal)
            || !string.Equals(binding.UserMessage.RecordId, binding.MessageId, StringComparison.Ordinal))
        {
            throw new InvalidDataException("The Local Agent Main Chat projection identity is inconsistent.");
        }
        if (tasks.Select(task => task.TaskId).Distinct(StringComparer.Ordinal).Count() != tasks.Count
            || tasks.Any(task =>
                !string.Equals(task.SourceThreadId, threadId, StringComparison.Ordinal)
                || !string.Equals(task.SourceTurnId, binding.TurnId, StringComparison.Ordinal)
                || !string.Equals(task.ProjectId, run.ProjectId, StringComparison.Ordinal)))
        {
            throw new InvalidDataException("The Local Agent Main Chat task projection identity is inconsistent.");
        }
        return new LocalAgentMainChatTurn(run, binding, detail, tasks);
    }

    private static void ValidateCreated(
        LocalAgentConversationScope scope,
        string turnId,
        string modelConfigId,
        LocalAgentRunCreatedResponse created)
    {
        var run = created.Run;
        if (string.IsNullOrWhiteSpace(created.OperationId)
            || string.IsNullOrWhiteSpace(run.RunId)
            || !string.Equals(run.ProfileKey, "main_chat", StringComparison.Ordinal)
            || !string.Equals(run.OwnerUserId, scope.AccountId, StringComparison.Ordinal)
            || !string.Equals(run.OwnerEntityType, "conversation", StringComparison.Ordinal)
            || !string.Equals(run.OwnerEntityId, scope.ThreadId, StringComparison.Ordinal)
            || !string.Equals(run.ProjectId, scope.ProjectId, StringComparison.Ordinal)
            || !string.Equals(run.ModelConfigId, modelConfigId, StringComparison.Ordinal)
            || IsTerminal(run.Status))
        {
            throw new InvalidDataException($"The Local Agent returned an invalid Main Chat run for turn '{turnId}'.");
        }
    }

    private static void ValidateScope(LocalAgentConversationScope scope)
    {
        ArgumentNullException.ThrowIfNull(scope);
        RequireIdentity(scope.AccountId, nameof(scope.AccountId));
        RequireIdentity(scope.ThreadId, nameof(scope.ThreadId));
        if (scope.ProjectId is { } projectId) RequireIdentity(projectId, nameof(scope.ProjectId));
        if (scope.ContactAgentId is { } agentId) RequireIdentity(agentId, nameof(scope.ContactAgentId));
    }

    private static void RequireIdentity(string? value, string name)
    {
        if (string.IsNullOrWhiteSpace(value)
            || value != value.Trim()
            || value.Any(char.IsControl))
        {
            throw new ArgumentException("A Local Agent Main Chat identity is invalid.", name);
        }
    }

    private static bool IsTerminal(LocalAgentRunStatus status) => status is
        LocalAgentRunStatus.Succeeded or LocalAgentRunStatus.Failed or LocalAgentRunStatus.Cancelled;

    private void OnProjectionChanged(object? sender, WindowsLocalAgentProjectionSnapshot e) =>
        ProjectionChanged?.Invoke(this, EventArgs.Empty);

    private void OnProjectionCleared(object? sender, EventArgs e) =>
        ProjectionCleared?.Invoke(this, EventArgs.Empty);
}
