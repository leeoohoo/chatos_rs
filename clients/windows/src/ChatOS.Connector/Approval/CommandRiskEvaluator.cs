namespace ChatOS.Connector.Approval;

public sealed class CommandRiskEvaluator
{
    private static readonly string[] HighRiskMarkers =
    [
        "rm -rf", "del /s", "del /q", "rmdir /s", "rd /s", "format ", "diskpart",
        "mkfs", "dd if=", "shutdown", "restart-computer", "stop-computer", "reboot",
        "kill -9", "taskkill /f", "git reset --hard", "git clean -fd", "reg delete",
        "bcdedit", "curl | sh", "curl | bash", "invoke-expression", "encodedcommand",
        "chmod -r 777", "> /dev/",
    ];

    private static readonly string[] MediumRiskMarkers =
    [
        " rm ", " remove-item ", " mv ", " move-item ", " cp ", " copy-item ",
        "install ", "npm install", "pnpm install", "yarn add", "cargo install",
        "dotnet tool install", "git commit", "git push", "chmod ", "chown ",
        "mkdir ", "new-item ", "touch ", "set-content ", "add-content ",
        "out-file ", "winget install", "choco install", "scoop install",
    ];

    public ConnectorApprovalRisk Evaluate(string command, IReadOnlyList<string> arguments)
    {
        var text = $" {CommandDisplay.Format(command, arguments).ToLowerInvariant()} ";
        var highRiskMarker = HighRiskMarkers.FirstOrDefault(text.Contains);
        if (highRiskMarker is not null)
        {
            return new ConnectorApprovalRisk(
                ConnectorApprovalRiskLevel.High,
                $"Command contains a high-risk operation: {highRiskMarker.Trim()}");
        }

        if (MediumRiskMarkers.Any(text.Contains))
        {
            return new ConnectorApprovalRisk(
                ConnectorApprovalRiskLevel.Medium,
                "Command may modify files, dependencies, processes, or remote state.");
        }

        return new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.Low);
    }
}
