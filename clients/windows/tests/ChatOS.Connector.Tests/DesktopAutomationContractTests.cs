using System.Xml.Linq;

namespace ChatOS.Connector.Tests;

public sealed class DesktopAutomationContractTests
{
    private static readonly string[] RequiredAutomationIds =
    [
        "ChatOS.Login.Root",
        "ChatOS.Login.Username",
        "ChatOS.Login.Password",
        "ChatOS.Login.Submit",
        "ChatOS.Shell.Root",
        "ChatOS.Shell.Notepad",
        "ChatOS.Shell.Artifacts",
        "ChatOS.Shell.AccountMenu",
        "ChatOS.Shell.Settings",
        "ChatOS.Shell.Refresh",
        "ChatOS.Shell.CreateResource",
        "ChatOS.Shell.Contacts",
        "ChatOS.Shell.Projects",
        "ChatOS.Shell.LocalResources",
        "ChatOS.Shell.RemoteResources",
        "ChatOS.Shell.Workspace",
        "ChatOS.Settings.Root",
        "ChatOS.Settings.Back",
        "ChatOS.Settings.Language",
        "ChatOS.Settings.Theme",
        "ChatOS.Settings.FontScale",
        "ChatOS.Settings.PetEnabled",
        "ChatOS.Settings.SandboxEnabled",
        "ChatOS.Settings.SandboxProfile",
        "ChatOS.Settings.SandboxNetwork",
        "ChatOS.Settings.SandboxReadinessRefresh",
        "ChatOS.Settings.SandboxSave",
        "ChatOS.Settings.ApprovalMode",
        "ChatOS.Settings.PluginsRefresh",
        "ChatOS.Chat.Root",
        "ChatOS.Chat.ModelPicker",
        "ChatOS.Chat.Reasoning",
        "ChatOS.Chat.PlanMode",
        "ChatOS.Chat.Refresh",
        "ChatOS.Chat.Timeline",
        "ChatOS.Chat.AddAttachment",
        "ChatOS.Chat.Composer",
        "ChatOS.Chat.Stop",
        "ChatOS.Chat.Send",
        "ChatOS.Chat.TaskDetail",
        "ChatOS.Workspace.Navigation",
        "ChatOS.Workspace.Chat",
        "ChatOS.Workspace.Files",
        "ChatOS.Workspace.Git",
        "ChatOS.Workspace.Plan",
        "ChatOS.Workspace.Run",
        "ChatOS.Workspace.Content",
        "ChatOS.Pet.Root",
        "ChatOS.Pet.HitTarget",
        "ChatOS.Pet.ActivityToast",
        "ChatOS.Pet.Approval",
        "ChatOS.Pet.Approval.Deny",
        "ChatOS.Pet.Approval.AllowOnce",
        "ChatOS.Pet.Approval.AllowSession",
        "ChatOS.Pet.DecisionToast",
        "ChatOS.Pet.Inbox",
        "ChatOS.Pet.ActivityList",
        "ChatOS.Pet.ActivityDetail",
        "ChatOS.Pet.QuickChat",
        "ChatOS.Pet.QuickChat.Back",
        "ChatOS.Pet.QuickChat.Resources",
        "ChatOS.Pet.QuickChat.Conversation",
        "ChatOS.Pet.QuickChat.Composer",
        "ChatOS.Pet.QuickChat.Stop",
        "ChatOS.Pet.QuickChat.Send",
        "ChatOS.Approval.Root",
        "ChatOS.Approval.Deny",
        "ChatOS.Approval.AllowOnce",
        "ChatOS.Approval.AllowSession",
        "ChatOS.LocalTerminal.Root",
        "ChatOS.LocalTerminal.Back",
        "ChatOS.LocalTerminal.Stop",
        "ChatOS.LocalTerminal.Input",
        "ChatOS.LocalTerminal.Send",
    ];

    private static readonly string[] RequiredExplicitAccessibleNames =
    [
        "ChatOS.Login.Root",
        "ChatOS.Login.Username",
        "ChatOS.Login.Password",
        "ChatOS.Login.Submit",
        "ChatOS.Shell.Root",
        "ChatOS.Shell.Notepad",
        "ChatOS.Shell.Artifacts",
        "ChatOS.Shell.AccountMenu",
        "ChatOS.Shell.Refresh",
        "ChatOS.Shell.CreateResource",
        "ChatOS.Shell.Contacts",
        "ChatOS.Shell.Projects",
        "ChatOS.Shell.LocalResources",
        "ChatOS.Shell.RemoteResources",
        "ChatOS.Settings.Root",
        "ChatOS.Settings.Back",
        "ChatOS.Settings.Language",
        "ChatOS.Settings.Theme",
        "ChatOS.Settings.FontScale",
        "ChatOS.Settings.PetEnabled",
        "ChatOS.Settings.SandboxEnabled",
        "ChatOS.Settings.SandboxProfile",
        "ChatOS.Settings.SandboxNetwork",
        "ChatOS.Settings.ApprovalMode",
        "ChatOS.Chat.Root",
        "ChatOS.Chat.ModelPicker",
        "ChatOS.Chat.Refresh",
        "ChatOS.Chat.AddAttachment",
        "ChatOS.Chat.Composer",
        "ChatOS.Pet.Root",
        "ChatOS.Pet.HitTarget",
        "ChatOS.Pet.QuickChat.Back",
        "ChatOS.Pet.QuickChat.Composer",
        "ChatOS.LocalTerminal.Root",
        "ChatOS.LocalTerminal.Back",
        "ChatOS.LocalTerminal.Input",
    ];

    [Fact]
    public void DesktopXamlAutomationIdsAreUniqueAndComplete()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var occurrences = Directory
            .EnumerateFiles(desktopRoot, "*.xaml", SearchOption.AllDirectories)
            .SelectMany(path => XDocument.Load(path, LoadOptions.SetLineInfo)
                .Root!
                .DescendantsAndSelf()
                .SelectMany(element => element.Attributes()
                    .Where(attribute => attribute.Name.LocalName == "AutomationProperties.AutomationId")
                    .Select(attribute => new AutomationIdOccurrence(attribute.Value, path, element))))
            .ToArray();

        var duplicateIds = occurrences
            .GroupBy(occurrence => occurrence.Id, StringComparer.Ordinal)
            .Where(group => group.Count() > 1)
            .Select(group => $"{group.Key}: {string.Join(", ", group.Select(item => Path.GetFileName(item.Path)))}")
            .ToArray();
        Assert.True(duplicateIds.Length == 0, $"Duplicate AutomationId values: {string.Join("; ", duplicateIds)}");

        var actualIds = occurrences.Select(occurrence => occurrence.Id).ToHashSet(StringComparer.Ordinal);
        var missingIds = RequiredAutomationIds.Where(id => !actualIds.Contains(id)).ToArray();
        Assert.True(missingIds.Length == 0, $"Missing required AutomationId values: {string.Join(", ", missingIds)}");

        var missingNames = RequiredExplicitAccessibleNames
            .Where(id => occurrences
                .Where(occurrence => occurrence.Id == id)
                .Select(occurrence => occurrence.Element)
                .All(element => element.Attributes().All(attribute =>
                    attribute.Name.LocalName != "AutomationProperties.Name" ||
                    string.IsNullOrWhiteSpace(attribute.Value))))
            .ToArray();
        Assert.True(missingNames.Length == 0, $"Automation elements missing explicit accessible names: {string.Join(", ", missingNames)}");
    }

    private static string FindRepositoryRoot()
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null)
        {
            if (File.Exists(Path.Combine(directory.FullName, "ChatOS.Win.sln")))
            {
                return directory.FullName;
            }

            directory = directory.Parent;
        }

        throw new DirectoryNotFoundException("Could not locate the ChatOS Windows repository root.");
    }

    private sealed record AutomationIdOccurrence(string Id, string Path, XElement Element);
}
