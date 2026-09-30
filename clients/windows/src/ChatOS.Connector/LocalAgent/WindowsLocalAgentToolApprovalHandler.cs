using System.Text.Json;
using ChatOS.Connector.Approval;
using ChatOS.Connector.Workspaces;

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
            value.ToolName is "project_write" or "terminal_exec");
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

        var outcome = await approvals.RequestAsync(new CommandApprovalRequest(
            invocation.InvocationId,
            ownerUserId,
            deviceId,
            context.Workspace.Id,
            presentation.Command,
            presentation.Arguments,
            presentation.WorkingDirectory,
            "Local Agent Task",
            $"{presentation.Scope}:{context.ConversationId}"),
            presentation.Risk,
            cancellationToken).ConfigureAwait(false);
        await DecideAsync(
            ownerUserId,
            invocation,
            outcome.Approved,
            Limit(outcome.Reason, 4_000),
            cancellationToken).ConfigureAwait(false);
        return true;
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

        var path = WindowsLocalAgentProjectToolExecutor.RequiredString(
            invocation.Arguments, "path");
        return new ApprovalPresentation(
            "project_write",
            [path],
            context.ProjectRoot,
            new ConnectorApprovalRisk(
                ConnectorApprovalRiskLevel.Medium,
                "The local task requests permission to modify a project file."),
            "local-agent-project-write");
    }

    private static string Limit(string value, int maximum) =>
        value.Length <= maximum ? value : value[..maximum];

    private sealed record ApprovalPresentation(
        string Command,
        IReadOnlyList<string> Arguments,
        string WorkingDirectory,
        ConnectorApprovalRisk Risk,
        string Scope);
}
