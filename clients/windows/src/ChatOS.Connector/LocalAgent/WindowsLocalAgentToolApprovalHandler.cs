using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Approval;
using ChatOS.Connector.Workspaces;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.LocalAgent;

internal sealed record ListPendingLocalToolApprovalsCommand(
    string Type, string OwnerUserId, uint Limit);
internal sealed record ListPendingLocalToolApprovalsResult(
    string Type, IReadOnlyList<WindowsLocalToolInvocation> Invocations);
internal sealed record DecideLocalToolApprovalCommand(
    string Type,
    string OwnerUserId,
    string InvocationId,
    ulong ExpectedVersion,
    string Decision,
    string DecidedBy,
    string Reason);
internal sealed record DecideLocalToolApprovalResult(string Type, JsonElement Result);

public sealed class WindowsLocalAgentToolApprovalHandler(
    ILocalAgentHostClient host,
    WindowsLocalAgentProjectToolExecutor projectTools,
    IConnectorWorkspaceContext workspaces,
    CommandApprovalCoordinator approvals,
    CommandRiskEvaluator riskEvaluator)
{
    private const string ReviewerId = "windows-local-approval";
    private static readonly TimeSpan RunStatusPollInterval = TimeSpan.FromMilliseconds(250);

    internal async Task<bool> ResolveNextPendingAsync(
        string ownerUserId,
        CancellationToken cancellationToken)
    {
        var result = await host.SendAsync<
            ListPendingLocalToolApprovalsCommand,
            ListPendingLocalToolApprovalsResult>(
            new("list_pending_tool_approvals", ownerUserId, 20), cancellationToken)
            .ConfigureAwait(false);
        if (result.Type != "pending_tool_approvals")
        {
            throw new InvalidDataException("Invalid Local Agent approval result.");
        }

        var invocation = result.Invocations.FirstOrDefault(value =>
            value.ToolName is
                "project_write" or "project_copy" or "project_move" or "terminal_exec" or
                "remote_connection_controller_run_command" or
                "remote_connection_controller_upload_file");
        if (invocation is null) return false;

        WindowsLocalAgentProjectContext context;
        ApprovalPresentation presentation;
        try
        {
            context = await projectTools.ResolveContextAsync(
                ownerUserId, invocation.RunId, cancellationToken).ConfigureAwait(false);
            presentation = Presentation(invocation, context);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch
        {
            await DecideAsync(
                ownerUserId,
                invocation,
                false,
                "The local project context is unavailable.",
                cancellationToken).ConfigureAwait(false);
            return true;
        }

        var deviceId = workspaces.DeviceId;
        if (string.IsNullOrWhiteSpace(deviceId))
        {
            await DecideAsync(
                ownerUserId,
                invocation,
                false,
                "The local Connector is not paired.",
                cancellationToken).ConfigureAwait(false);
            return true;
        }

        var request = new CommandApprovalRequest(
            invocation.InvocationId,
            ownerUserId,
            deviceId,
            context.Workspace.Id,
            presentation.Command,
            presentation.Arguments,
            presentation.WorkingDirectory,
            "Local Agent Task",
            $"{presentation.Scope}:{context.ConversationId}");
        var outcome = await WaitForApprovalWhileRunIsActionableAsync(
            token => approvals.RequestAsync(request, presentation.Risk, token),
            async token =>
            {
                var run = await host.SendAsync<GetLocalRunCommand, GetLocalRunResult>(
                    new("get_run", ownerUserId, invocation.RunId), token).ConfigureAwait(false);
                if (run.Type != "run" ||
                    !string.Equals(run.Run.RunId, invocation.RunId, StringComparison.Ordinal) ||
                    !string.Equals(run.Run.OwnerUserId, ownerUserId, StringComparison.Ordinal))
                {
                    throw new InvalidDataException("Invalid Local Agent Run result.");
                }
                return run.Run.Status;
            },
            RunStatusPollInterval,
            cancellationToken).ConfigureAwait(false);
        if (outcome is null)
        {
            // Cancelling a conversation Run also fails its still-open Host invocations. Do not
            // send a stale approval decision after that authoritative transition.
            return true;
        }
        await DecideAsync(
            ownerUserId,
            invocation,
            outcome.Approved,
            Limit(outcome.Reason, 4_000),
            cancellationToken).ConfigureAwait(false);
        return true;
    }

    internal static async Task<ConnectorApprovalOutcome?> WaitForApprovalWhileRunIsActionableAsync(
        Func<CancellationToken, Task<ConnectorApprovalOutcome>> requestApproval,
        Func<CancellationToken, Task<string>> readRunStatus,
        TimeSpan pollInterval,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(requestApproval);
        ArgumentNullException.ThrowIfNull(readRunStatus);
        if (pollInterval <= TimeSpan.Zero)
        {
            throw new ArgumentOutOfRangeException(nameof(pollInterval));
        }

        using var approvalLifetime =
            CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var approvalTask = requestApproval(approvalLifetime.Token);
        var runMonitorTask = MonitorRunAsync(
            readRunStatus, pollInterval, approvalLifetime.Token);
        var completed = await Task.WhenAny(approvalTask, runMonitorTask).ConfigureAwait(false);
        if (completed == approvalTask)
        {
            try
            {
                return await approvalTask.ConfigureAwait(false);
            }
            finally
            {
                approvalLifetime.Cancel();
                await IgnoreMonitorCancellationAsync(runMonitorTask).ConfigureAwait(false);
            }
        }

        try
        {
            await runMonitorTask.ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch
        {
            // A lost or invalid Host Run cannot safely receive an approval. Fail closed and
            // remove the local pending card instead of leaving a stale action behind.
        }

        approvalLifetime.Cancel();
        try
        {
            _ = await approvalTask.ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (approvalLifetime.IsCancellationRequested)
        {
        }
        cancellationToken.ThrowIfCancellationRequested();
        return null;
    }

    private static async Task MonitorRunAsync(
        Func<CancellationToken, Task<string>> readRunStatus,
        TimeSpan pollInterval,
        CancellationToken cancellationToken)
    {
        while (true)
        {
            await Task.Delay(pollInterval, cancellationToken).ConfigureAwait(false);
            var status = await readRunStatus(cancellationToken).ConfigureAwait(false);
            if (!string.Equals(status, "waiting_tool_result", StringComparison.Ordinal))
            {
                return;
            }
        }
    }

    private static async Task IgnoreMonitorCancellationAsync(Task monitor)
    {
        try
        {
            await monitor.ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
        }
    }

    private async Task DecideAsync(
        string ownerUserId,
        WindowsLocalToolInvocation invocation,
        bool approve,
        string reason,
        CancellationToken cancellationToken)
    {
        var result = await host.SendAsync<
            DecideLocalToolApprovalCommand,
            DecideLocalToolApprovalResult>(new(
            "decide_tool_approval",
            ownerUserId,
            invocation.InvocationId,
            invocation.Version,
            approve ? "approve" : "reject",
            ReviewerId,
            reason), cancellationToken).ConfigureAwait(false);
        if (result.Type != "tool_approval")
        {
            throw new InvalidDataException("Invalid Local Agent approval decision result.");
        }
    }

    private ApprovalPresentation Presentation(
        WindowsLocalToolInvocation invocation,
        WindowsLocalAgentProjectContext context)
    {
        if (invocation.ToolName == "remote_connection_controller_run_command")
        {
            if (string.IsNullOrWhiteSpace(context.RemoteConnectionId))
            {
                throw new InvalidOperationException(
                    "The Local Agent Task has no program-bound remote connection.");
            }
            var command = WindowsLocalAgentProjectToolExecutor.RequiredString(
                invocation.Arguments, "command");
            var workingDirectory = WindowsLocalAgentProjectToolExecutor.OptionalString(
                invocation.Arguments, "working_directory") ?? "~";
            return new ApprovalPresentation(
                command,
                [],
                workingDirectory,
                riskEvaluator.Evaluate(command, []),
                "local-agent-remote-command");
        }

        if (invocation.ToolName == "remote_connection_controller_upload_file")
        {
            if (string.IsNullOrWhiteSpace(context.RemoteConnectionId))
            {
                throw new InvalidOperationException(
                    "The Local Agent Task has no program-bound remote connection.");
            }
            var remotePath = WindowsLocalAgentProjectToolExecutor.RequiredString(
                invocation.Arguments, "path");
            return new ApprovalPresentation(
                "remote_upload_file",
                [remotePath],
                "remote",
                new ConnectorApprovalRisk(
                    ConnectorApprovalRiskLevel.Medium,
                    "The local task requests permission to upload a remote file."),
                "local-agent-remote-upload");
        }

        if (invocation.ToolName == "terminal_exec")
        {
            var command = WindowsLocalAgentProjectToolExecutor.RequiredString(
                invocation.Arguments, "command");
            var arguments = WindowsLocalAgentProjectToolExecutor.StringArray(
                invocation.Arguments, "arguments", 100);
            var workingDirectory = new WorkspacePathGuard(context.ProjectRoot).ResolveExisting(
                WindowsLocalAgentProjectToolExecutor.OptionalString(
                    invocation.Arguments, "working_directory") ?? ".");
            if (!Directory.Exists(workingDirectory))
            {
                throw new InvalidOperationException(
                    "The Local Agent terminal working directory is invalid.");
            }
            return new ApprovalPresentation(
                command,
                arguments,
                workingDirectory,
                riskEvaluator.Evaluate(command, arguments),
                "local-agent-terminal");
        }

        if (invocation.ToolName is "project_copy" or "project_move")
        {
            return ProjectMutationPresentation(invocation, context.ProjectRoot);
        }

        return ProjectMutationPresentation(invocation, context.ProjectRoot);
    }

    internal static ApprovalPresentation ProjectMutationPresentation(
        WindowsLocalToolInvocation invocation,
        string projectRoot)
    {
        if (invocation.ToolName is "project_copy" or "project_move")
        {
            var sourcePath = WindowsLocalAgentProjectToolExecutor.RequiredString(
                invocation.Arguments, "source_path");
            var targetPath = WindowsLocalAgentProjectToolExecutor.RequiredString(
                invocation.Arguments, "target_path");
            var replaceExisting = WindowsLocalAgentProjectToolExecutor.OptionalBool(
                invocation.Arguments, "replace_existing") ?? false;
            var isCopy = invocation.ToolName == "project_copy";
            return new ApprovalPresentation(
                invocation.ToolName,
                [
                    sourcePath,
                    targetPath,
                    $"--replace-existing={replaceExisting.ToString().ToLowerInvariant()}",
                ],
                projectRoot,
                new ConnectorApprovalRisk(
                    replaceExisting
                        ? ConnectorApprovalRiskLevel.High
                        : ConnectorApprovalRiskLevel.Medium,
                    replaceExisting
                        ? isCopy
                            ? "The local task requests permission to copy a project entry and replace the target if it exists."
                            : "The local task requests permission to move or rename a project entry and replace the target if it exists."
                        : isCopy
                            ? "The local task requests permission to copy a project entry without replacing an existing target."
                            : "The local task requests permission to move or rename a project entry without replacing an existing target."),
                isCopy ? "local-agent-project-copy" : "local-agent-project-move");
        }

        if (invocation.ToolName != "project_write")
        {
            throw new InvalidOperationException("Unsupported Local Agent project approval.");
        }

        var path = WindowsLocalAgentProjectToolExecutor.RequiredString(
            invocation.Arguments, "path");
        var content = WindowsLocalAgentProjectToolExecutor.RequiredStringAllowEmpty(
            invocation.Arguments, "content");
        var createOnly = WindowsLocalAgentProjectToolExecutor.OptionalBool(
            invocation.Arguments, "create_only") ?? false;
        var contentBytes = Encoding.UTF8.GetBytes(content);
        var contentSha256 = Convert.ToHexString(SHA256.HashData(contentBytes)).ToLowerInvariant();
        return new ApprovalPresentation(
            "project_write",
            [
                path,
                $"--create-only={createOnly.ToString().ToLowerInvariant()}",
                $"--content-utf8-bytes={contentBytes.Length}",
                $"--content-sha256={contentSha256}",
            ],
            projectRoot,
            new ConnectorApprovalRisk(
                ConnectorApprovalRiskLevel.Medium,
                createOnly
                    ? "The local task requests permission to create a project file with the displayed content fingerprint."
                    : "The local task requests permission to create or replace a project file with the displayed content fingerprint."),
            "local-agent-project-write");
    }

    private static string Limit(string value, int maximum) =>
        value.Length <= maximum ? value : value[..maximum];

    internal sealed record ApprovalPresentation(
        string Command,
        IReadOnlyList<string> Arguments,
        string WorkingDirectory,
        ConnectorApprovalRisk Risk,
        string Scope);
}
