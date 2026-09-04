using System.Collections.Concurrent;
using System.Text.Json;

namespace ChatOS.Connector.Plugins;

public interface IPluginRuntimeLifetime
{
    Task TerminateAllAsync();
}

internal sealed class PluginRuntimeSessionStore : IPluginRuntimeLifetime
{
    private readonly ConcurrentDictionary<string, Session> _sessions = new(StringComparer.Ordinal);
    private readonly ConcurrentDictionary<string, SkillSession> _skillSessions = new(StringComparer.Ordinal);
    private readonly SemaphoreSlim _exclusiveExecution = new(1, 1);

    public async Task InsertAsync(
        PluginRuntimeIdentity identity,
        IPluginMcpClient client,
        IReadOnlyList<JsonElement> tools,
        IReadOnlySet<string> permissionSnapshot,
        bool requiresExclusiveExecution,
        string workingDirectory,
        string artifactDirectory,
        string visualSessionDirectory,
        string displayName)
    {
        var session = new Session(
            identity,
            client,
            tools.Select(tool => tool.Clone()).ToArray(),
            permissionSnapshot.ToHashSet(StringComparer.Ordinal),
            requiresExclusiveExecution,
            workingDirectory,
            artifactDirectory,
            visualSessionDirectory,
            displayName,
            null,
            null);
        if (_sessions.TryGetValue(identity.AdapterSessionId, out var previous))
        {
            _sessions[identity.AdapterSessionId] = session;
            await previous.Client.TerminateAsync().ConfigureAwait(false);
            await previous.Client.DisposeAsync().ConfigureAwait(false);
        }
        else if (!_sessions.TryAdd(identity.AdapterSessionId, session))
        {
            await client.TerminateAsync().ConfigureAwait(false);
            await client.DisposeAsync().ConfigureAwait(false);
            throw new PluginRuntimeException("Plugin runtime session could not be registered.");
        }
    }

    public PluginRuntimeIdentity Validate(
        string adapterSessionId,
        string pluginId,
        string releaseId,
        string artifactSha256,
        string componentKey,
        string? workspaceId,
        string? projectId = null)
    {
        if (!_sessions.TryGetValue(adapterSessionId, out var session))
        {
            throw new PluginRuntimeException("Plugin local session does not exist or has ended.");
        }

        var identity = session.Identity;
        if (!string.Equals(identity.PluginId, pluginId, StringComparison.Ordinal) ||
            !string.Equals(identity.ReleaseId, releaseId, StringComparison.Ordinal) ||
            !string.Equals(identity.ArtifactSha256, artifactSha256, StringComparison.OrdinalIgnoreCase) ||
            !string.Equals(identity.ComponentKey, componentKey, StringComparison.Ordinal) ||
            !string.Equals(identity.WorkspaceId, workspaceId, StringComparison.Ordinal) ||
            !string.Equals(identity.ProjectId, projectId, StringComparison.Ordinal))
        {
            throw new PluginRuntimeException("Plugin request does not match its prepared session.");
        }

        return identity;
    }

    public void InsertSkill(
        PluginRuntimeIdentity identity,
        JsonElement expectedSnapshot,
        DateTimeOffset expiresAt)
    {
        var session = new SkillSession(identity, expectedSnapshot.Clone(), expiresAt);
        if (!_skillSessions.TryAdd(identity.AdapterSessionId, session))
        {
            throw new PluginRuntimeException("Plugin Skill runtime session could not be registered.");
        }
    }

    public JsonElement ValidateSkill(
        string adapterSessionId,
        string pluginId,
        string releaseId,
        string artifactSha256,
        string componentKey,
        string? workspaceId,
        string? projectId = null)
    {
        if (!_skillSessions.TryGetValue(adapterSessionId, out var session) ||
            session.ExpiresAt <= DateTimeOffset.UtcNow)
        {
            _skillSessions.TryRemove(adapterSessionId, out _);
            throw new PluginRuntimeException("Plugin Skill session does not exist or has ended.");
        }
        var identity = session.Identity;
        if (!string.Equals(identity.PluginId, pluginId, StringComparison.Ordinal) ||
            !string.Equals(identity.ReleaseId, releaseId, StringComparison.Ordinal) ||
            !string.Equals(identity.ArtifactSha256, artifactSha256, StringComparison.OrdinalIgnoreCase) ||
            !string.Equals(identity.ComponentKey, componentKey, StringComparison.Ordinal) ||
            !string.Equals(identity.WorkspaceId, workspaceId, StringComparison.Ordinal) ||
            !string.Equals(identity.ProjectId, projectId, StringComparison.Ordinal))
        {
            throw new PluginRuntimeException("Plugin Skill request does not match its prepared session.");
        }
        return session.ExpectedSnapshot.Clone();
    }

    public bool RemoveSkill(
        string adapterSessionId,
        string runId,
        string? workspaceId,
        string? projectId)
    {
        if (!_skillSessions.TryGetValue(adapterSessionId, out var session))
        {
            return false;
        }
        if (!string.Equals(session.Identity.RunId, runId, StringComparison.Ordinal) ||
            !string.Equals(session.Identity.WorkspaceId, workspaceId, StringComparison.Ordinal) ||
            !string.Equals(session.Identity.ProjectId, projectId, StringComparison.Ordinal))
        {
            throw new PluginRuntimeException("Plugin Skill cancellation does not match its prepared session.");
        }
        return _skillSessions.TryRemove(adapterSessionId, out _);
    }

    public JsonElement ToolDefinition(string adapterSessionId, string toolName)
    {
        if (!_sessions.TryGetValue(adapterSessionId, out var session))
        {
            throw new PluginRuntimeException("Plugin local session does not exist or has ended.");
        }

        var tool = session.Tools.FirstOrDefault(candidate =>
            candidate.ValueKind == JsonValueKind.Object &&
            candidate.TryGetProperty("name", out var name) &&
            name.ValueKind == JsonValueKind.String &&
            string.Equals(name.GetString(), toolName, StringComparison.Ordinal));
        return tool.ValueKind == JsonValueKind.Undefined
            ? throw new PluginRuntimeException("Plugin MCP did not publish this tool.")
            : tool.Clone();
    }

    public IReadOnlySet<string> Permissions(string adapterSessionId) =>
        _sessions.TryGetValue(adapterSessionId, out var session)
            ? session.PermissionSnapshot
            : throw new PluginRuntimeException("Plugin local session does not exist or has ended.");

    public string WorkingDirectory(string adapterSessionId) =>
        _sessions.TryGetValue(adapterSessionId, out var session)
            ? session.WorkingDirectory
            : throw new PluginRuntimeException("Plugin local session does not exist or has ended.");

    public string ArtifactDirectory(string adapterSessionId) =>
        _sessions.TryGetValue(adapterSessionId, out var session)
            ? session.ArtifactDirectory
            : throw new PluginRuntimeException("Plugin local session does not exist or has ended.");

    public void BindVisualOwner(string adapterSessionId, PluginVisualSessionOwner owner)
    {
        while (_sessions.TryGetValue(adapterSessionId, out var session))
        {
            var updated = session with
            {
                VisualOwner = owner,
                VisualOwnerBoundAt = DateTimeOffset.UtcNow,
            };
            if (_sessions.TryUpdate(adapterSessionId, updated, session))
            {
                return;
            }
        }
    }

    internal IReadOnlyList<PluginVisualDescriptor> VisualDescriptors() => _sessions.Values
        .Where(session => session.VisualOwner is not null &&
            session.VisualOwnerBoundAt is not null &&
            !string.IsNullOrWhiteSpace(session.VisualSessionDirectory))
        .Select(session => new PluginVisualDescriptor(
            session.Identity,
            session.DisplayName,
            session.VisualSessionDirectory,
            session.VisualOwner!,
            session.VisualOwnerBoundAt!.Value))
        .OrderByDescending(value => value.OwnerBoundAt)
        .ToArray();

    public async Task<JsonElement> CallAsync(
        string adapterSessionId,
        string invocationId,
        string toolName,
        JsonElement arguments,
        TimeSpan timeout,
        CancellationToken cancellationToken)
    {
        if (!_sessions.TryGetValue(adapterSessionId, out var session))
        {
            throw new PluginRuntimeException("Plugin local session does not exist or has ended.");
        }

        var invocationCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        if (!session.ActiveInvocations.TryAdd(invocationId, invocationCancellation))
        {
            invocationCancellation.Dispose();
            throw new PluginRuntimeException("Plugin invocation id is already active.");
        }

        var exclusive = false;
        try
        {
            if (session.RequiresExclusiveExecution)
            {
                await _exclusiveExecution.WaitAsync(cancellationToken).ConfigureAwait(false);
                exclusive = true;
            }

            try
            {
                return await session.Client.CallToolAsync(
                        toolName,
                        arguments,
                        timeout,
                        invocationCancellation.Token)
                    .ConfigureAwait(false);
            }
            catch (OperationCanceledException exception) when (!cancellationToken.IsCancellationRequested)
            {
                throw new PluginRuntimeException("Plugin MCP invocation was cancelled.", exception);
            }
        }
        finally
        {
            if (session.ActiveInvocations.TryRemove(invocationId, out var removed))
            {
                removed.Dispose();
            }
            if (exclusive)
            {
                _exclusiveExecution.Release();
            }
        }
    }

    public async Task<string> CancelAsync(
        string adapterSessionId,
        string? invocationId,
        string? workspaceId,
        string? projectId = null)
    {
        if (!_sessions.TryGetValue(adapterSessionId, out var session))
        {
            return "invocation_not_found";
        }

        if (!string.Equals(session.Identity.WorkspaceId, workspaceId, StringComparison.Ordinal) ||
            !string.Equals(session.Identity.ProjectId, projectId, StringComparison.Ordinal))
        {
            throw new PluginRuntimeException("Plugin cancellation does not match the prepared workspace.");
        }

        if (invocationId is not null)
        {
            if (!session.ActiveInvocations.TryGetValue(invocationId, out var invocation))
            {
                return "invocation_not_found";
            }

            invocation.Cancel();
            return "cancel_requested";
        }

        if (_sessions.TryRemove(adapterSessionId, out session))
        {
            foreach (var invocation in session.ActiveInvocations.Values)
            {
                invocation.Cancel();
            }

            await session.Client.TerminateAsync().ConfigureAwait(false);
            await session.Client.DisposeAsync().ConfigureAwait(false);
            return "cancelled";
        }

        return "invocation_not_found";
    }

    public async Task TerminateAllAsync()
    {
        var sessions = _sessions.ToArray();
        _sessions.Clear();
        _skillSessions.Clear();
        foreach (var session in sessions)
        {
            foreach (var invocation in session.Value.ActiveInvocations.Values)
            {
                invocation.Cancel();
            }

            await session.Value.Client.TerminateAsync().ConfigureAwait(false);
            await session.Value.Client.DisposeAsync().ConfigureAwait(false);
        }
    }

    public async Task TerminatePluginAsync(string pluginId)
    {
        foreach (var value in _skillSessions.Where(value =>
                     string.Equals(value.Value.Identity.PluginId, pluginId, StringComparison.Ordinal)).ToArray())
        {
            _skillSessions.TryRemove(value.Key, out _);
        }
        var matching = _sessions
            .Where(value => string.Equals(value.Value.Identity.PluginId, pluginId, StringComparison.Ordinal))
            .ToArray();
        foreach (var value in matching)
        {
            if (!_sessions.TryRemove(value.Key, out var session))
            {
                continue;
            }

            foreach (var invocation in session.ActiveInvocations.Values)
            {
                invocation.Cancel();
            }

            await session.Client.TerminateAsync().ConfigureAwait(false);
            await session.Client.DisposeAsync().ConfigureAwait(false);
        }
    }

    private sealed record Session(
        PluginRuntimeIdentity Identity,
        IPluginMcpClient Client,
        IReadOnlyList<JsonElement> Tools,
        IReadOnlySet<string> PermissionSnapshot,
        bool RequiresExclusiveExecution,
        string WorkingDirectory,
        string ArtifactDirectory,
        string VisualSessionDirectory,
        string DisplayName,
        PluginVisualSessionOwner? VisualOwner,
        DateTimeOffset? VisualOwnerBoundAt)
    {
        public ConcurrentDictionary<string, CancellationTokenSource> ActiveInvocations { get; } =
            new(StringComparer.Ordinal);
    }

    private sealed record SkillSession(
        PluginRuntimeIdentity Identity,
        JsonElement ExpectedSnapshot,
        DateTimeOffset ExpiresAt);

    internal sealed record PluginVisualDescriptor(
        PluginRuntimeIdentity Identity,
        string DisplayName,
        string VisualSessionDirectory,
        PluginVisualSessionOwner Owner,
        DateTimeOffset OwnerBoundAt);
}
