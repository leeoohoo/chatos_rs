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
        "ChatOS.Login.PetSprite",
        "ChatOS.Shell.Root",
        "ChatOS.Shell.Notepad",
        "ChatOS.Shell.Artifacts",
        "ChatOS.Shell.AccountMenu",
        "ChatOS.Shell.Settings",
        "ChatOS.Shell.SignOut",
        "ChatOS.Shell.Refresh",
        "ChatOS.Shell.CreateResource",
        "ChatOS.Shell.CreateProject",
        "ChatOS.Shell.CreateTerminal",
        "ChatOS.Shell.Contacts",
        "ChatOS.Shell.Projects",
        "ChatOS.Shell.Applications",
        "ChatOS.Shell.LocalResources",
        "ChatOS.Shell.RemoteResources",
        "ChatOS.Shell.Workspace",
        "ChatOS.CreateProject.Root",
        "ChatOS.CreateProject.Workspace",
        "ChatOS.CreateProject.NoWorkspace",
        "ChatOS.CreateProject.Parent",
        "ChatOS.CreateProject.CurrentLocation",
        "ChatOS.CreateProject.AbsolutePath",
        "ChatOS.CreateProject.EntrySummary",
        "ChatOS.CreateProject.ToggleHidden",
        "ChatOS.CreateProject.NewFolder",
        "ChatOS.CreateProject.Refresh",
        "ChatOS.CreateProject.NewFolderName",
        "ChatOS.CreateProject.CreateFolder",
        "ChatOS.CreateProject.CancelFolder",
        "ChatOS.CreateProject.NewFolderError",
        "ChatOS.CreateProject.Folders",
        "ChatOS.CreateProject.Loading",
        "ChatOS.CreateProject.Empty",
        "ChatOS.CreateProject.Name",
        "ChatOS.CreateProject.Description",
        "ChatOS.CreateProject.Type",
        "ChatOS.CreateProject.TypeDescription",
        "ChatOS.CreateProject.Error",
        "ChatOS.AuxiliaryOverlay.Root",
        "ChatOS.Notepad.Root",
        "ChatOS.Notepad.Search",
        "ChatOS.Notepad.Refresh",
        "ChatOS.Notepad.Close",
        "ChatOS.Notepad.Copy",
        "ChatOS.Notepad.Folders",
        "ChatOS.Notepad.Notes",
        "ChatOS.Settings.Root",
        "ChatOS.Settings.Back",
        "ChatOS.Settings.Navigation",
        "ChatOS.Settings.Nav.General",
        "ChatOS.Settings.Nav.WeChat",
        "ChatOS.Settings.Nav.Pet",
        "ChatOS.Settings.Nav.Utilities",
        "ChatOS.Settings.Nav.Models",
        "ChatOS.Settings.Nav.Connector",
        "ChatOS.Settings.Nav.Plugins",
        "ChatOS.Settings.Nav.Approvals",
        "ChatOS.Settings.Nav.Runtime",
        "ChatOS.Settings.Nav.Access",
        "ChatOS.Settings.WeChatGenerateCode",
        "ChatOS.Settings.Language",
        "ChatOS.Settings.Theme",
        "ChatOS.Settings.FontScale",
        "ChatOS.Settings.PetEnabled",
        "ChatOS.Settings.PetSize",
        "ChatOS.Settings.PetResetPosition",
        "ChatOS.Settings.PetShowProcess",
        "ChatOS.Settings.PetShowCompletions",
        "ChatOS.Settings.GlobalUtilitiesEnabled",
        "ChatOS.Settings.GlobalScreenshot",
        "ChatOS.Settings.GlobalRecording",
        "ChatOS.Settings.GlobalClipboard",
        "ChatOS.Settings.GlobalQuickSearch",
        "ChatOS.Settings.SandboxEnabled",
        "ChatOS.Settings.SandboxProfile",
        "ChatOS.Settings.SandboxNetwork",
        "ChatOS.Settings.SandboxReadinessRefresh",
        "ChatOS.Settings.SandboxSave",
        "ChatOS.Settings.ApprovalMode",
        "ChatOS.Settings.PluginsRefresh",
        "ChatOS.Settings.CustomPluginSourcePath",
        "ChatOS.Settings.CustomPluginChooseFolder",
        "ChatOS.Settings.CustomPluginInspect",
        "ChatOS.Settings.CustomPluginInstall",
        "ChatOS.Settings.CustomPluginNpmMode",
        "ChatOS.Settings.CustomPluginNpmPackage",
        "ChatOS.Settings.CustomPluginNpmVersion",
        "ChatOS.Settings.CustomPluginNpmBinary",
        "ChatOS.Settings.CustomPluginNpmComponent",
        "ChatOS.Settings.CustomPluginNpmDisplayName",
        "ChatOS.Settings.CustomPluginNpmArguments",
        "ChatOS.Settings.CustomPluginNpmDescription",
        "ChatOS.Settings.CustomPluginNpmInspect",
        "ChatOS.Chat.Root",
        "ChatOS.Chat.LiveProcesses",
        "ChatOS.Chat.ModelPicker",
        "ChatOS.Chat.Reasoning",
        "ChatOS.Chat.Refresh",
        "ChatOS.Chat.Timeline",
        "ChatOS.Chat.Empty",
        "ChatOS.Chat.Error",
        "ChatOS.Chat.AttachmentError",
        "ChatOS.Chat.ComposerSurface",
        "ChatOS.Chat.AddAttachment",
        "ChatOS.Chat.Composer",
        "ChatOS.Chat.Stop",
        "ChatOS.Chat.Send",
        "ChatOS.Chat.TaskDetail",
        "ChatOS.Chat.AskUser.SingleChoice",
        "ChatOS.Chat.AskUser.Cancel",
        "ChatOS.Chat.AskUser.Submit",
        "ChatOS.Workspace.Root",
        "ChatOS.Workspace.Navigation",
        "ChatOS.Workspace.Chat",
        "ChatOS.Workspace.ProjectChatPreparing",
        "ChatOS.Workspace.ProjectChatError",
        "ChatOS.Workspace.ProjectChatErrorMessage",
        "ChatOS.Workspace.ProjectChatRetry",
        "ChatOS.Workspace.Files",
        "ChatOS.Workspace.Git",
        "ChatOS.Workspace.Run",
        "ChatOS.Workspace.AgentTeam",
        "ChatOS.Workspace.RequirementSurveys",
        "ChatOS.Workspace.Content",
        "ChatOS.ProjectFiles.Root",
        "ChatOS.ProjectFiles.Search",
        "ChatOS.ProjectFiles.SearchSubmit",
        "ChatOS.ProjectFiles.Refresh",
        "ChatOS.ProjectFiles.ToggleView",
        "ChatOS.ProjectFiles.DirectoryList",
        "ChatOS.ProjectFiles.Parent",
        "ChatOS.ProjectFiles.Loading",
        "ChatOS.ProjectFiles.DirectoryEmpty",
        "ChatOS.ProjectFiles.Tree",
        "ChatOS.ProjectFiles.FileSearchResults",
        "ChatOS.ProjectFiles.ContentSearchResults",
        "ChatOS.ProjectFiles.ClearSearch",
        "ChatOS.ProjectFiles.SearchProgress",
        "ChatOS.ProjectFiles.SearchEmpty",
        "ChatOS.ProjectFiles.SelectedFileName",
        "ChatOS.ProjectFiles.SelectedFilePath",
        "ChatOS.ProjectFiles.TextPreview",
        "ChatOS.ProjectFiles.Editor",
        "ChatOS.ProjectFiles.ActionStatus",
        "ChatOS.ProjectFiles.Error",
        "ChatOS.ProjectGit.Root",
        "ChatOS.ProjectGit.Head",
        "ChatOS.ProjectGit.Sync",
        "ChatOS.ProjectGit.Refresh",
        "ChatOS.ProjectGit.Cancel",
        "ChatOS.ProjectGit.NotRepository",
        "ChatOS.ProjectGit.Initialize",
        "ChatOS.ProjectGit.Changes",
        "ChatOS.ProjectGit.ChangesEmpty",
        "ChatOS.ProjectGit.DiffPath",
        "ChatOS.ProjectGit.DiffContent",
        "ChatOS.ProjectGit.DiffEmpty",
        "ChatOS.ProjectGit.ActionStatus",
        "ChatOS.ProjectGit.Error",
        "ChatOS.RequirementSurveys.Root",
        "ChatOS.RequirementSurveys.Refresh",
        "ChatOS.RequirementSurveys.List",
        "ChatOS.RequirementSurveys.Submit",
        "ChatOS.AgentTeam.Root",
        "ChatOS.AgentTeam.Navigation",
        "ChatOS.AgentTeam.OpenRequirementSurveys",
        "ChatOS.AgentTeam.Members",
        "ChatOS.AgentTeam.Assets.Empty",
        "ChatOS.AgentWorkspace.Root",
        "ChatOS.AgentWorkspace.Management",
        "ChatOS.AgentWorkspace.Documents",
        "ChatOS.AgentWorkspace.DirectChats",
        "ChatOS.AgentWorkspace.ManagementPage",
        "ChatOS.AgentWorkspace.DocumentsPage",
        "ChatOS.Applications.Catalog",
        "ChatOS.Applications.Refresh",
        "ChatOS.Applications.Grid",
        "ChatOS.Applications.OpenPluginManagement",
        "ChatOS.Applications.Host",
        "ChatOS.Applications.Back",
        "ChatOS.Applications.Reload",
        "ChatOS.Applications.ProjectPicker",
        "ChatOS.Applications.SharedProject",
        "ChatOS.Applications.WebView",
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
        "ChatOS.Approval.Source",
        "ChatOS.Approval.Risk",
        "ChatOS.Approval.Command",
        "ChatOS.Approval.WorkingDirectory",
        "ChatOS.Approval.Reason",
        "ChatOS.Approval.Queue",
        "ChatOS.Approval.Deny",
        "ChatOS.Approval.AllowOnce",
        "ChatOS.Approval.AllowSession",
        "ChatOS.LocalTerminal.Root",
        "ChatOS.LocalTerminal.Back",
        "ChatOS.LocalTerminal.New",
        "ChatOS.LocalTerminal.Stop",
        "ChatOS.LocalTerminal.Search",
        "ChatOS.LocalTerminal.FindPrevious",
        "ChatOS.LocalTerminal.Find",
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
        "ChatOS.Shell.SignOut",
        "ChatOS.Shell.Refresh",
        "ChatOS.Shell.CreateResource",
        "ChatOS.Shell.CreateProject",
        "ChatOS.Shell.CreateTerminal",
        "ChatOS.Shell.Contacts",
        "ChatOS.Shell.Projects",
        "ChatOS.Shell.Applications",
        "ChatOS.Shell.LocalResources",
        "ChatOS.Shell.RemoteResources",
        "ChatOS.Settings.Root",
        "ChatOS.Settings.Back",
        "ChatOS.Settings.Nav.General",
        "ChatOS.Settings.Nav.WeChat",
        "ChatOS.Settings.Nav.Pet",
        "ChatOS.Settings.Nav.Utilities",
        "ChatOS.Settings.Nav.Models",
        "ChatOS.Settings.Nav.Connector",
        "ChatOS.Settings.Nav.Plugins",
        "ChatOS.Settings.Nav.Approvals",
        "ChatOS.Settings.Nav.Runtime",
        "ChatOS.Settings.Nav.Access",
        "ChatOS.Settings.WeChatGenerateCode",
        "ChatOS.Settings.Language",
        "ChatOS.Settings.Theme",
        "ChatOS.Settings.FontScale",
        "ChatOS.Settings.PetEnabled",
        "ChatOS.Settings.PetSize",
        "ChatOS.Settings.PetResetPosition",
        "ChatOS.Settings.PetShowProcess",
        "ChatOS.Settings.PetShowCompletions",
        "ChatOS.Settings.GlobalUtilitiesEnabled",
        "ChatOS.Settings.GlobalScreenshot",
        "ChatOS.Settings.GlobalRecording",
        "ChatOS.Settings.GlobalClipboard",
        "ChatOS.Settings.GlobalQuickSearch",
        "ChatOS.Settings.SandboxEnabled",
        "ChatOS.Settings.SandboxProfile",
        "ChatOS.Settings.SandboxNetwork",
        "ChatOS.Settings.ApprovalMode",
        "ChatOS.Chat.Root",
        "ChatOS.Chat.ModelPicker",
        "ChatOS.Chat.Refresh",
        "ChatOS.Chat.AddAttachment",
        "ChatOS.Chat.Composer",
        "ChatOS.ProjectFiles.Root",
        "ChatOS.ProjectFiles.Search",
        "ChatOS.ProjectFiles.SearchSubmit",
        "ChatOS.ProjectFiles.Refresh",
        "ChatOS.ProjectFiles.DirectoryList",
        "ChatOS.ProjectFiles.Parent",
        "ChatOS.ProjectFiles.Loading",
        "ChatOS.ProjectFiles.DirectoryEmpty",
        "ChatOS.ProjectFiles.FileSearchResults",
        "ChatOS.ProjectFiles.ContentSearchResults",
        "ChatOS.ProjectFiles.SelectedFileName",
        "ChatOS.ProjectFiles.SelectedFilePath",
        "ChatOS.ProjectFiles.TextPreview",
        "ChatOS.ProjectFiles.Editor",
        "ChatOS.ProjectFiles.ActionStatus",
        "ChatOS.ProjectFiles.Error",
        "ChatOS.ProjectGit.Root",
        "ChatOS.ProjectGit.Head",
        "ChatOS.ProjectGit.Sync",
        "ChatOS.ProjectGit.Refresh",
        "ChatOS.ProjectGit.Cancel",
        "ChatOS.ProjectGit.NotRepository",
        "ChatOS.ProjectGit.Initialize",
        "ChatOS.ProjectGit.Changes",
        "ChatOS.ProjectGit.ChangesEmpty",
        "ChatOS.ProjectGit.DiffPath",
        "ChatOS.ProjectGit.DiffContent",
        "ChatOS.ProjectGit.DiffEmpty",
        "ChatOS.ProjectGit.ActionStatus",
        "ChatOS.ProjectGit.Error",
        "ChatOS.RequirementSurveys.Root",
        "ChatOS.RequirementSurveys.Refresh",
        "ChatOS.RequirementSurveys.List",
        "ChatOS.RequirementSurveys.Submit",
        "ChatOS.AgentTeam.Root",
        "ChatOS.AgentTeam.Navigation",
        "ChatOS.AgentTeam.OpenRequirementSurveys",
        "ChatOS.AgentTeam.Members",
        "ChatOS.Pet.Root",
        "ChatOS.Pet.HitTarget",
        "ChatOS.Pet.QuickChat.Back",
        "ChatOS.Pet.QuickChat.Composer",
        "ChatOS.LocalTerminal.Root",
        "ChatOS.LocalTerminal.Back",
        "ChatOS.LocalTerminal.New",
        "ChatOS.LocalTerminal.FindPrevious",
        "ChatOS.LocalTerminal.Input",
        "ChatOS.Applications.Catalog",
        "ChatOS.Applications.Grid",
        "ChatOS.Applications.OpenPluginManagement",
        "ChatOS.Applications.Host",
        "ChatOS.Applications.ProjectPicker",
        "ChatOS.Applications.SharedProject",
        "ChatOS.Applications.WebView",
    ];

    [Fact]
    public void DesktopXamlAutomationIdsAreUniqueAndComplete()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var occurrences = Directory
            .EnumerateFiles(desktopRoot, "*.xaml", SearchOption.AllDirectories)
            .Where(path => !path
                .Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar)
                .Any(segment => segment is "bin" or "obj"))
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

    [Fact]
    public void CreateProjectDialogProvidesStableActionAutomationIds()
    {
        var source = File.ReadAllText(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "MainWindow.xaml.cs"));

        Assert.Contains("\"PrimaryButton\"", source, StringComparison.Ordinal);
        Assert.Contains("\"ChatOS.CreateProject.Submit\"", source, StringComparison.Ordinal);
        Assert.Contains("\"CloseButton\"", source, StringComparison.Ordinal);
        Assert.Contains("\"ChatOS.CreateProject.Cancel\"", source, StringComparison.Ordinal);
        Assert.Contains("\"Cancel project creation\"", source, StringComparison.Ordinal);
        Assert.Contains("GetTemplateChild(templatePart) is not Button button", source, StringComparison.Ordinal);

        var dialogSource = File.ReadAllText(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "Features", "Projects", "CreateProjectDialogContent.xaml"));
        Assert.Contains("ChatOS.CreateProject.AbsolutePath", dialogSource, StringComparison.Ordinal);
        Assert.Contains("ChatOS.CreateProject.OpenAbsolutePath", dialogSource, StringComparison.Ordinal);
    }

    [Fact]
    public void NewApprovalMovesKeyboardFocusToTheSafeDecision()
    {
        var source = File.ReadAllText(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "MainWindow.xaml.cs"));

        Assert.Contains("var previousApprovalId = ActiveApproval?.Id;", source, StringComparison.Ordinal);
        Assert.Contains(
            "!string.Equals(previousApprovalId, approval.Id, StringComparison.Ordinal)",
            source,
            StringComparison.Ordinal);
        Assert.Contains("DeclineApprovalButton.Focus(FocusState.Programmatic)", source, StringComparison.Ordinal);
    }

    [Fact]
    public void ProjectWorkspaceAutomationExposesPathIdentitiesAndSelectedNavigationState()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var document = XDocument.Load(Path.Combine(
            desktopRoot, "AppShell", "WorkspaceHostPage.xaml"));
        var source = File.ReadAllText(Path.Combine(
            desktopRoot, "AppShell", "WorkspaceHostPage.xaml.cs"));

        var root = FindElementByAutomationId(document, "ChatOS.Workspace.Root");
        Assert.Equal(
            "{x:Bind ViewModel.SelectedResource.Subtitle, Mode=OneWay}",
            root.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.Name").Value);
        Assert.Equal(
            "{x:Bind ViewModel.Conversation.ConversationId, Mode=OneWay}",
            root.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.HelpText").Value);
        Assert.Equal(
            "{x:Bind ViewModel.SelectedResource.Id, Mode=OneWay}",
            root.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.ItemStatus").Value);
        Assert.Contains(
            "AutomationProperties.SetItemStatus(button, selected ? \"selected\" : \"not-selected\")",
            source,
            StringComparison.Ordinal);
    }

    [Fact]
    public void ProjectFilesAutomationExposesWorkspaceSelectionContentAndOutcome()
    {
        var document = XDocument.Load(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "Features", "Projects", "ProjectFilesPage.xaml"));
        var root = FindElementByAutomationId(document, "ChatOS.ProjectFiles.Root");

        Assert.Equal(
            "{x:Bind ViewModel.ProjectName, Mode=OneWay}",
            root.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.Name").Value);
        Assert.Equal(
            "{x:Bind ViewModel.ProjectRoot, Mode=OneWay}",
            root.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.HelpText").Value);
        Assert.Equal(
            "{x:Bind ViewModel.CurrentPath, Mode=OneWay}",
            root.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.ItemStatus").Value);

        var directory = FindElementByAutomationId(document, "ChatOS.ProjectFiles.DirectoryList");
        Assert.Equal("None", directory.Attribute("SelectionMode")?.Value);
        Assert.Equal("Collapsed", directory.Attribute("Visibility")?.Value);
        Assert.Equal(
            "{x:Bind ViewModel.CurrentPath, Mode=OneWay}",
            directory.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.ItemStatus").Value);
        var directoryItemStyle = directory.Descendants()
            .Single(element => element.Name.LocalName == "Style" &&
                element.Attribute("TargetType")?.Value == "ListViewItem");
        Assert.Contains(directoryItemStyle.Elements(), element =>
            element.Name.LocalName == "Setter" &&
            element.Attribute("Property")?.Value == "AutomationProperties.Name" &&
            element.Attribute("Value")?.Value == "{Binding Name}");
        Assert.Contains(directoryItemStyle.Elements(), element =>
            element.Name.LocalName == "Setter" &&
            element.Attribute("Property")?.Value == "AutomationProperties.HelpText" &&
            element.Attribute("Value")?.Value == "{Binding Path}");
        var entryButton = FindElementByAutomationId(document, "ChatOS.ProjectFiles.Entry");
        Assert.Equal("OnDirectoryEntryClick", entryButton.Attribute("Click")?.Value);
        Assert.Equal("{x:Bind Name}", entryButton.Attributes().Single(attribute =>
            attribute.Name.LocalName == "AutomationProperties.Name").Value);
        Assert.Equal("{x:Bind Path}", entryButton.Attributes().Single(attribute =>
            attribute.Name.LocalName == "AutomationProperties.HelpText").Value);
        var treeEntryButton = FindElementByAutomationId(document, "ChatOS.ProjectFiles.TreeEntry");
        Assert.Equal("OnTreeEntryClick", treeEntryButton.Attribute("Click")?.Value);
        Assert.Equal("{Binding Content.Name}", treeEntryButton.Attributes().Single(attribute =>
            attribute.Name.LocalName == "AutomationProperties.Name").Value);
        Assert.Equal("{Binding Content.Path}", treeEntryButton.Attributes().Single(attribute =>
            attribute.Name.LocalName == "AutomationProperties.HelpText").Value);
        Assert.Equal("{Binding Content}", treeEntryButton.Attribute("Tag")?.Value);
        Assert.Equal("Stretch", treeEntryButton.Attribute("HorizontalAlignment")?.Value);
        Assert.Equal("36", treeEntryButton.Attribute("MinHeight")?.Value);
        var tree = FindElementByAutomationId(document, "ChatOS.ProjectFiles.Tree");
        var treeItemStyle = tree.Descendants()
            .Single(element => element.Name.LocalName == "Style" &&
                element.Attribute("TargetType")?.Value == "TreeViewItem");
        Assert.Contains(treeItemStyle.Elements(), element =>
            element.Name.LocalName == "Setter" &&
            element.Attribute("Property")?.Value == "HorizontalContentAlignment" &&
            element.Attribute("Value")?.Value == "Stretch");
        Assert.Contains(treeItemStyle.Elements(), element =>
            element.Name.LocalName == "Setter" &&
            element.Attribute("Property")?.Value == "MinHeight" &&
            element.Attribute("Value")?.Value == "38");
        var empty = FindElementByAutomationId(document, "ChatOS.ProjectFiles.DirectoryEmpty");
        Assert.Equal(
            "{x:Bind ViewModel.HasEmptyDirectoryState, Mode=OneWay, Converter={StaticResource BooleanToVisibilityConverter}}",
            empty.Attribute("Visibility")?.Value);
        Assert.Equal(
            "{x:Bind ViewModel.CurrentPath, Mode=OneWay}",
            empty.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.HelpText").Value);
        Assert.Equal(
            "{x:Bind ViewModel.SelectedFileDisplayPath, Mode=OneWay}",
            FindElementByAutomationId(document, "ChatOS.ProjectFiles.SelectedFilePath")
                .Attribute("Text")?.Value);
        Assert.Equal(
            "{x:Bind ViewModel.SelectedFile.Content, Mode=OneWay}",
            FindElementByAutomationId(document, "ChatOS.ProjectFiles.TextPreview")
                .Attribute("Text")?.Value);
        Assert.Equal(
            "{x:Bind ViewModel.EntryActionMessage, Mode=OneWay}",
            FindElementByAutomationId(document, "ChatOS.ProjectFiles.ActionStatus")
                .Attribute("Text")?.Value);
        Assert.Equal(
            "{x:Bind ViewModel.ErrorMessage, Mode=OneWay}",
            FindElementByAutomationId(document, "ChatOS.ProjectFiles.Error")
                .Attribute("Text")?.Value);
    }

    [Fact]
    public void PetApprovalActionsRemainReadableAndMoveAwayFromTheComposer()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var document = XDocument.Load(Path.Combine(
            desktopRoot, "Features", "Pet", "PetWindow.xaml"));
        var source = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "Pet", "PetWindow.xaml.cs"));

        foreach (var automationId in new[]
                 {
                     "ChatOS.Pet.Approval.Deny",
                     "ChatOS.Pet.Approval.AllowSession",
                     "ChatOS.Pet.Approval.AllowOnce",
                 })
        {
            var button = FindElementByAutomationId(document, automationId);
            Assert.Equal("Stretch", button.Attribute("HorizontalAlignment")?.Value);
            Assert.Equal("Center", button.Attribute("HorizontalContentAlignment")?.Value);
            Assert.Equal("38", button.Attribute("MinHeight")?.Value);
            Assert.NotNull(button.Attributes().SingleOrDefault(attribute =>
                attribute.Name.LocalName == "AutomationProperties.Name"));
        }

        Assert.Contains("private const int ApprovalWidth = 460;", source, StringComparison.Ordinal);
        Assert.Contains("MoveApprovalToSafeTopRight();", source, StringComparison.Ordinal);
        Assert.Contains("RestorePreApprovalPlacement();", source, StringComparison.Ordinal);
    }

    [Fact]
    public void ProjectGitAutomationExposesWorkspaceDiffAndOutcome()
    {
        var document = XDocument.Load(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "Features", "Projects", "ProjectGitPage.xaml"));
        var root = FindElementByAutomationId(document, "ChatOS.ProjectGit.Root");

        Assert.Equal(
            "{x:Bind ViewModel.ProjectName, Mode=OneWay}",
            root.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.Name").Value);
        Assert.Equal(
            "{x:Bind ViewModel.ProjectRoot, Mode=OneWay}",
            root.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.HelpText").Value);
        Assert.Equal(
            "{x:Bind ViewModel.RepositoryRoot, Mode=OneWay}",
            root.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.ItemStatus").Value);
        Assert.Equal(
            "{x:Bind ViewModel.SelectedDiff.Path, Mode=OneWay}",
            FindElementByAutomationId(document, "ChatOS.ProjectGit.DiffPath")
                .Attribute("Text")?.Value);
        Assert.Equal(
            "{x:Bind ViewModel.SelectedDiff.Content, Mode=OneWay}",
            FindElementByAutomationId(document, "ChatOS.ProjectGit.DiffContent")
                .Attribute("Text")?.Value);
        Assert.Equal(
            "{x:Bind ViewModel.ActionMessage, Mode=OneWay}",
            FindElementByAutomationId(document, "ChatOS.ProjectGit.ActionStatus")
                .Attribute("Text")?.Value);
        Assert.Equal(
            "{x:Bind ViewModel.ErrorMessage, Mode=OneWay}",
            FindElementByAutomationId(document, "ChatOS.ProjectGit.Error")
                .Attribute("Text")?.Value);
    }

    [Fact]
    public void ConversationAutomationExposesIdentityStateMessagesProcessesAndErrors()
    {
        var document = XDocument.Load(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "Features", "Chat", "ConversationPage.xaml"));
        var root = FindElementByAutomationId(document, "ChatOS.Chat.Root");

        Assert.Equal(
            "{x:Bind ViewModel.AutomationIdentity, Mode=OneWay}",
            root.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.HelpText").Value);
        Assert.Equal(
            "{x:Bind ViewModel.AutomationStatus, Mode=OneWay}",
            root.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.ItemStatus").Value);

        var dynamicIds = document.Descendants()
            .SelectMany(element => element.Attributes())
            .Where(attribute => attribute.Name.LocalName == "AutomationProperties.AutomationId")
            .Select(attribute => attribute.Value)
            .ToHashSet(StringComparer.Ordinal);
        Assert.Contains("{x:Bind AutomationId}", dynamicIds);
        Assert.Contains("{x:Bind UserMessageAutomationId}", dynamicIds);
        Assert.Contains("{x:Bind GuidanceAutomationId}", dynamicIds);
        Assert.Contains("{x:Bind AssistantAutomationId}", dynamicIds);
        Assert.Contains("{x:Bind LiveAutomationId}", dynamicIds);
        Assert.Contains("{x:Bind HistoryAutomationId}", dynamicIds);
        Assert.Contains("{x:Bind ProcessToggleAutomationId}", dynamicIds);
        Assert.Contains("{x:Bind TaskGraphAutomationId}", dynamicIds);

        Assert.Equal(
            "{x:Bind ViewModel.ErrorMessage, Mode=OneWay}",
            FindElementByAutomationId(document, "ChatOS.Chat.Error").Attribute("Text")?.Value);
        Assert.Equal(
            "{x:Bind ViewModel.AttachmentError, Mode=OneWay}",
            FindElementByAutomationId(document, "ChatOS.Chat.AttachmentError").Attribute("Text")?.Value);
    }

    [Fact]
    public void SidebarListsDoNotWriteNullSelectionsBackAndAlwaysHandleItemClicks()
    {
        var mainWindow = XDocument.Load(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "MainWindow.xaml"));
        var sidebarLists = mainWindow.Descendants()
            .Where(element => element.Name.LocalName == "ListView")
            .Where(element => element.Attributes().Any(attribute =>
                attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
                attribute.Value.StartsWith("ChatOS.Shell.", StringComparison.Ordinal)))
            .ToArray();

        Assert.Equal(5, sidebarLists.Length);
        Assert.All(sidebarLists, list =>
        {
            Assert.Contains("Mode=OneWay", list.Attribute("SelectedItem")?.Value);
            Assert.Equal("OnSidebarSelectionChanged", list.Attribute("SelectionChanged")?.Value);
            Assert.Equal("True", list.Attribute("IsItemClickEnabled")?.Value);
            Assert.Equal("OnSidebarItemClicked", list.Attribute("ItemClick")?.Value);
        });
    }

    [Fact]
    public void GeneralAndPetSettingsUseIndependentPageSections()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var document = XDocument.Load(Path.Combine(
            desktopRoot, "Features", "Settings", "SettingsPage.xaml"));
        var source = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "Settings", "SettingsPage.xaml.cs"));

        var namedElements = document.Descendants()
            .Where(element => element.Attribute(XName.Get("Name", "http://schemas.microsoft.com/winfx/2006/xaml")) is not null)
            .ToDictionary(
                element => element.Attribute(XName.Get("Name", "http://schemas.microsoft.com/winfx/2006/xaml"))!.Value,
                StringComparer.Ordinal);

        Assert.Contains("GeneralSettingsSection", namedElements.Keys);
        Assert.Contains("PetSettingsSection", namedElements.Keys);
        Assert.Equal("Collapsed", namedElements["PetSettingsSection"].Attribute("Visibility")?.Value);
        Assert.Contains(namedElements["GeneralSettingsSection"].Descendants(), element =>
            element.Attribute(XName.Get("Name", "http://schemas.microsoft.com/winfx/2006/xaml"))?.Value == "LanguagePicker");
        Assert.DoesNotContain(namedElements["GeneralSettingsSection"].Descendants(), element =>
            element.Attribute(XName.Get("Name", "http://schemas.microsoft.com/winfx/2006/xaml"))?.Value == "PetEnabledSwitch");
        Assert.Contains(namedElements["PetSettingsSection"].Descendants(), element =>
            element.Attribute(XName.Get("Name", "http://schemas.microsoft.com/winfx/2006/xaml"))?.Value == "PetEnabledSwitch");
        Assert.Contains("GeneralSettingsSection.Visibility = Visibility.Visible;", source, StringComparison.Ordinal);
        Assert.Contains("PetSettingsSection.Visibility = Visibility.Visible;", source, StringComparison.Ordinal);
    }

    [Fact]
    public void CustomPluginInstallerIsScopedToThePluginSettingsSection()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var document = XDocument.Load(Path.Combine(
            desktopRoot, "Features", "Settings", "SettingsPage.xaml"));
        var source = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "Settings", "SettingsPage.xaml.cs"));

        var card = document.Descendants().Single(element =>
            element.Attribute(XName.Get("Name", "http://schemas.microsoft.com/winfx/2006/xaml"))?.Value ==
            "CustomPluginInstallerCard");
        Assert.Contains(card.Descendants(), element => element.Attributes().Any(attribute =>
            attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
            attribute.Value == "ChatOS.Settings.CustomPluginInspect"));
        Assert.Contains(card.Descendants(), element => element.Attributes().Any(attribute =>
            attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
            attribute.Value == "ChatOS.Settings.CustomPluginGitMode"));
        Assert.Contains(card.Descendants(), element => element.Attributes().Any(attribute =>
            attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
            attribute.Value == "ChatOS.Settings.CustomPluginGitUrl"));
        Assert.Contains(card.Descendants(), element => element.Attributes().Any(attribute =>
            attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
            attribute.Value == "ChatOS.Settings.CustomPluginGitReference"));
        Assert.Contains(card.Descendants(), element => element.Attributes().Any(attribute =>
            attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
            attribute.Value == "ChatOS.Settings.CustomPluginGitSubdirectory"));
        Assert.Contains(card.Descendants(), element => element.Attributes().Any(attribute =>
            attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
            attribute.Value == "ChatOS.Settings.CustomPluginGitInspect"));
        Assert.Contains("CustomPluginInstallerCard,", source, StringComparison.Ordinal);
        Assert.Contains(
            "CustomPluginInstallerCard.Visibility = Visibility.Visible;",
            source,
            StringComparison.Ordinal);
    }

    [Fact]
    public void DedicatedPluginInstallerConversationDisplaysItsCapabilityBoundary()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var document = XDocument.Load(Path.Combine(
            desktopRoot, "Features", "Chat", "ConversationPage.xaml"));
        var banner = FindElementByAutomationId(
            document, "ChatOS.Chat.PluginInstallerBoundary");

        Assert.Equal(
            "{x:Bind ViewModel.IsPluginInstaller, Mode=OneWay, Converter={StaticResource BooleanToVisibilityConverter}}",
            banner.Attribute("Visibility")?.Value);
        Assert.Contains(banner.Descendants(), element =>
            element.Name.LocalName == "TextBlock" &&
            element.Attribute("Text")?.Value.Contains(
                "PluginInstallerBoundaryText", StringComparison.Ordinal) == true);
    }

    [Fact]
    public void ProjectReentryUsesExplicitIndicatorsThatCannotRetainStaleSelectorState()
    {
        var source = File.ReadAllText(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "AppShell", "WorkspaceHostPage.xaml.cs"));
        var xaml = File.ReadAllText(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "AppShell", "WorkspaceHostPage.xaml"));

        Assert.Contains("SetDirectorySelection(\"files\");", source, StringComparison.Ordinal);
        Assert.Contains("SetWorkspaceSelection(\"directory\");", source, StringComparison.Ordinal);
        Assert.Contains("DirectoryIndicator.Visibility =", source, StringComparison.Ordinal);
        Assert.Contains("FilesIndicator.Visibility =", source, StringComparison.Ordinal);
        Assert.DoesNotContain("<SelectorBar", xaml, StringComparison.Ordinal);
    }

    [Fact]
    public void ContactWorkspaceMatchesTheMacSidebarAndOmitsProjectNavigation()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var mainWindow = XDocument.Load(Path.Combine(desktopRoot, "MainWindow.xaml"));
        var contacts = FindElementByAutomationId(mainWindow, "ChatOS.Shell.Contacts");
        Assert.Equal("{StaticResource ContactResourceTemplate}",
            contacts.Attribute("ItemTemplate")?.Value);

        var contactTemplate = mainWindow.Descendants().Single(element =>
            element.Attributes().Any(attribute =>
                attribute.Name.LocalName == "Key" && attribute.Value == "ContactResourceTemplate"));
        Assert.Contains(contactTemplate.Descendants(), element =>
            element.Name.LocalName == "FontIcon" && element.Attribute("Glyph")?.Value == "\uE724");

        var workspaceHost = XDocument.Load(Path.Combine(
            desktopRoot, "AppShell", "WorkspaceHostPage.xaml"));
        Assert.Contains(workspaceHost.Descendants(), element =>
            element.Attributes().Any(attribute =>
                attribute.Name.LocalName == "Name" && attribute.Value == "WorkspaceNavigationRow"));
        Assert.Contains(workspaceHost.Descendants(), element =>
            element.Attributes().Any(attribute =>
                attribute.Name.LocalName == "Name" && attribute.Value == "WorkspaceNavigationHost"));

        var source = File.ReadAllText(Path.Combine(
            desktopRoot, "AppShell", "WorkspaceHostPage.xaml.cs"));
        Assert.Contains("WorkspaceNavigationRow.Height = isProject ? new GridLength(44) : new GridLength(0);",
            source, StringComparison.Ordinal);
        Assert.Contains("WorkspaceNavigationHost.Visibility = isProject ? Visibility.Visible : Visibility.Collapsed;",
            source, StringComparison.Ordinal);
    }

    [Fact]
    public void ProjectAndTerminalCreationRoutesStayIndependent()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var mainWindow = XDocument.Load(Path.Combine(desktopRoot, "MainWindow.xaml"));
        var source = File.ReadAllText(Path.Combine(desktopRoot, "MainWindow.xaml.cs"));

        Assert.Equal(
            "OnCreateLocalProjectClicked",
            FindElementByAutomationId(mainWindow, "ChatOS.Shell.CreateProject").Attribute("Click")?.Value);
        Assert.Equal(
            "OnCreateLocalTerminalClicked",
            FindElementByAutomationId(mainWindow, "ChatOS.Shell.CreateTerminal").Attribute("Click")?.Value);

        var projectHandler = SliceMethod(source, "private async void OnCreateLocalProjectClicked", "private async void OnCreateLocalTerminalClicked");
        Assert.Contains("new CreateProjectViewModel(", projectHandler, StringComparison.Ordinal);
        Assert.Contains("ViewModel.CreateProjectAsync(", projectHandler, StringComparison.Ordinal);
        Assert.DoesNotContain("CreateLocalTerminalResource", projectHandler, StringComparison.Ordinal);

        var terminalHandler = SliceMethod(source, "private async void OnCreateLocalTerminalClicked", "private async Task ShowConnectorSetupRequiredAsync");
        Assert.Contains("ViewModel.CreateLocalTerminalResource(selected);", terminalHandler, StringComparison.Ordinal);
        Assert.DoesNotContain("CreateProjectViewModel", terminalHandler, StringComparison.Ordinal);
        Assert.DoesNotContain("ViewModel.CreateProjectAsync(", terminalHandler, StringComparison.Ordinal);
    }

    [Fact]
    public void ProjectCreationDialogKeepsFolderActionsReachableAndAutomationAddressable()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var dialog = XDocument.Load(Path.Combine(
            desktopRoot, "Features", "Projects", "CreateProjectDialogContent.xaml"));
        var source = File.ReadAllText(Path.Combine(desktopRoot, "MainWindow.xaml.cs"));
        var dialogSource = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "Projects", "CreateProjectDialogContent.xaml.cs"));

        Assert.Equal("ChatOS.CreateProject.Root",
            dialog.Root!.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.AutomationId").Value);
        Assert.Contains(dialog.Descendants(), element =>
            element.Attributes().Any(attribute =>
                attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
                attribute.Value == "ChatOS.CreateProject.CreateFolder"));
        Assert.Contains(dialog.Descendants(), element =>
            element.Attributes().Any(attribute =>
                attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
                attribute.Value == "ChatOS.CreateProject.CancelFolder"));
        _ = FindElementByAutomationId(dialog, "ChatOS.CreateProject.NewFolderError");
        _ = FindElementByAutomationId(dialog, "ChatOS.CreateProject.CurrentLocation");
        _ = FindElementByAutomationId(dialog, "ChatOS.CreateProject.AbsolutePath");
        _ = FindElementByAutomationId(dialog, "ChatOS.CreateProject.EntrySummary");
        _ = FindElementByAutomationId(dialog, "ChatOS.CreateProject.Loading");
        _ = FindElementByAutomationId(dialog, "ChatOS.CreateProject.Empty");
        _ = FindElementByAutomationId(dialog, "ChatOS.CreateProject.Error");
        Assert.Contains("ViewModel.HasUsableDirectoryListing &&", dialogSource, StringComparison.Ordinal);
        var folderList = FindElementByAutomationId(dialog, "ChatOS.CreateProject.Folders");
        var folderItemStyle = folderList.Elements().Single(element =>
            element.Name.LocalName == "ListView.ItemContainerStyle");
        var folderItemSetters = folderItemStyle.Descendants()
            .Where(element => element.Name.LocalName == "Setter")
            .ToDictionary(
                element => element.Attribute("Property")?.Value ?? string.Empty,
                element => element.Attribute("Value")?.Value ?? string.Empty,
                StringComparer.Ordinal);
        Assert.Equal("{Binding Name}", folderItemSetters["AutomationProperties.Name"]);
        Assert.Equal("{Binding Path}", folderItemSetters["AutomationProperties.HelpText"]);
        var newFolderAction = FindElementByAutomationId(
            dialog, "ChatOS.CreateProject.NewFolder");
        Assert.Equal("StackPanel", newFolderAction.Parent?.Name.LocalName);
        Assert.Equal("1", newFolderAction.Parent?.Parent?.Attribute("Grid.Row")?.Value);
        var form = dialog.Root!.Elements().Single(element =>
            element.Name.LocalName == "ScrollViewer").Elements().Single();
        Assert.Null(form.Attribute("Width"));
        Assert.Equal("700", form.Attribute("MaxWidth")?.Value);
        Assert.Equal("Stretch", form.Attribute("HorizontalAlignment")?.Value);

        var projectHandler = SliceMethod(
            source,
            "private async void OnCreateLocalProjectClicked",
            "private async void OnCreateLocalTerminalClicked");
        Assert.Contains("MinWidth = 760,", projectHandler, StringComparison.Ordinal);
        Assert.Contains("MaxWidth = 780,", projectHandler, StringComparison.Ordinal);
    }

    [Fact]
    public void BuiltInLocalResourceRoutesToTerminalInsteadOfSettings()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var viewModel = File.ReadAllText(Path.Combine(
            desktopRoot, "AppShell", "MainWindowViewModel.cs"));

        Assert.Contains("BuiltInLocalTerminalId = \"terminal-local\"", viewModel, StringComparison.Ordinal);
        Assert.Contains("WorkspaceResourceKind.LocalTerminal", viewModel, StringComparison.Ordinal);
        Assert.Contains("Localization.Text(\"本机终端\", \"Local Terminal\")", viewModel, StringComparison.Ordinal);
        Assert.Contains("Localization.Text(\"可用\", \"Available\")", viewModel, StringComparison.Ordinal);
        Assert.DoesNotContain(
            "status.DeviceName ?? Localization.Text(\"本机 Connector\", \"Local Connector\")",
            viewModel,
            StringComparison.Ordinal);
    }

    [Fact]
    public void WorkspaceContentStretchesEveryPageAcrossTheAvailableSurface()
    {
        var mainWindow = XDocument.Load(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "MainWindow.xaml"));
        var workspaceContent = mainWindow.Descendants().Single(element =>
            element.Attributes().Any(attribute =>
                attribute.Name.LocalName == "Name" && attribute.Value == "WorkspaceContent"));

        Assert.Equal("Stretch", workspaceContent.Attribute("HorizontalContentAlignment")?.Value);
        Assert.Equal("Stretch", workspaceContent.Attribute("VerticalContentAlignment")?.Value);

        var workspaceHost = XDocument.Load(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "AppShell", "WorkspaceHostPage.xaml"));
        var workspacePageContent = FindElementByAutomationId(
            workspaceHost, "ChatOS.Workspace.Content");
        Assert.Equal("Stretch", workspacePageContent.Attribute("HorizontalContentAlignment")?.Value);
        Assert.Equal("Stretch", workspacePageContent.Attribute("VerticalContentAlignment")?.Value);
    }

    [Fact]
    public void WorkspaceRootExposesProjectAndConversationIdentityToAutomation()
    {
        var workspaceHost = XDocument.Load(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "AppShell", "WorkspaceHostPage.xaml"));
        var root = FindElementByAutomationId(workspaceHost, "ChatOS.Workspace.Root");

        Assert.Equal(
            "{x:Bind ViewModel.SelectedResource.Id, Mode=OneWay}",
            root.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.ItemStatus").Value);
        Assert.Equal(
            "{x:Bind ViewModel.Conversation.ConversationId, Mode=OneWay}",
            root.Attributes().Single(attribute =>
                attribute.Name.LocalName == "AutomationProperties.HelpText").Value);
    }

    [Fact]
    public void EmptyWorkspaceHidesTheUnderlyingWorkspaceContent()
    {
        var source = File.ReadAllText(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "MainWindow.xaml.cs"));

        Assert.Contains(
            "WorkspaceContent.Visibility = isEmpty ? Visibility.Collapsed : Visibility.Visible;",
            source,
            StringComparison.Ordinal);
    }

    [Fact]
    public void ApplicationCatalogUsesTheAvailableWorkspaceWidth()
    {
        var catalog = XDocument.Load(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "Features", "Plugins", "PluginApplicationsPage.xaml"));
        var catalogScrollViewer = FindElementByAutomationId(catalog, "ChatOS.Applications.Catalog")
            .Descendants()
            .Single(element => element.Name.LocalName == "ScrollViewer");
        var catalogContent = catalogScrollViewer.Elements()
            .Single(element => element.Name.LocalName == "StackPanel");

        Assert.Equal("Stretch", catalogContent.Attribute("HorizontalAlignment")?.Value);
        Assert.Null(catalogContent.Attribute("MaxWidth"));
    }

    [Fact]
    public void ApplicationCatalogMatchesMacLoadingRefreshAndCardPresentation()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var document = XDocument.Load(Path.Combine(
            desktopRoot, "Features", "Plugins", "PluginApplicationsPage.xaml"));
        var refresh = FindElementByAutomationId(document, "ChatOS.Applications.Refresh");
        Assert.Contains(refresh.Descendants(), element =>
            element.Name.LocalName == "FontIcon" && element.Attribute("Glyph")?.Value == "\uE72C");
        Assert.Contains(document.Descendants(), element => element.Attributes().Any(attribute =>
            attribute.Name.LocalName == "Name" && attribute.Value == "CatalogLoadingState"));
        Assert.Contains(document.Descendants(), element => element.Attributes().Any(attribute =>
            attribute.Name.LocalName == "Name" && attribute.Value == "CatalogErrorState"));

        var xaml = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "Plugins", "PluginApplicationsPage.xaml"));
        Assert.Contains("MinHeight=\"47\" Text=\"{x:Bind Description}\"", xaml,
            StringComparison.Ordinal);

        var source = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "Plugins", "PluginApplicationsPage.xaml.cs"));
        var loadMethod = SliceMethod(source,
            "private async Task LoadApplicationsAsync", "private async void OnRefreshApplicationsClicked");
        Assert.Contains("var generation = ++_catalogGeneration;", loadMethod,
            StringComparison.Ordinal);
        Assert.Contains("accountGeneration != Shell.AccountGeneration", loadMethod,
            StringComparison.Ordinal);
        Assert.Contains("Shell.UpdatePluginApplicationCount(Applications.Count);", loadMethod,
            StringComparison.Ordinal);
        Assert.Equal(1, loadMethod.Split("Applications.Clear();", StringSplitOptions.None).Length - 1);
    }

    [Fact]
    public void ApplicationCatalogProvidesPluginManagementAndDeviceContextFallbacks()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var document = XDocument.Load(Path.Combine(
            desktopRoot, "Features", "Plugins", "PluginApplicationsPage.xaml"));
        Assert.Equal(
            "OnOpenPluginManagementClicked",
            FindElementByAutomationId(document, "ChatOS.Applications.OpenPluginManagement")
                .Attribute("Click")?.Value);
        Assert.Equal(
            "OnSharedProjectClicked",
            FindElementByAutomationId(document, "ChatOS.Applications.SharedProject")
                .Attribute("Click")?.Value);

        var applicationSource = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "Plugins", "PluginApplicationsPage.xaml.cs"));
        Assert.Contains("application.MissingContext, \"device\"", applicationSource,
            StringComparison.Ordinal);
        Assert.Contains("await LaunchAsync(null);", applicationSource, StringComparison.Ordinal);

        var windowSource = File.ReadAllText(Path.Combine(desktopRoot, "MainWindow.xaml.cs"));
        Assert.Contains("SettingsPage.OpenSection(\"plugins\")", windowSource,
            StringComparison.Ordinal);
    }

    [Fact]
    public void ConversationSwitchClearsTaskDetailAndRestoresTheLatestViewport()
    {
        var source = File.ReadAllText(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "Features", "Chat",
            "ConversationPage.xaml.cs"));

        Assert.Contains("nameof(ConversationSessionViewModel.ConversationId)", source,
            StringComparison.Ordinal);
        AssertAppearsInOrder(
            source,
            "TaskGraph.CloseCommand.Execute(null);",
            "_isPinnedToBottom = true;",
            "ScrollToBottom(false)");
    }

    [Fact]
    public void ProjectSettingsShowTheCompleteMacRunTargetMetadata()
    {
        var document = XDocument.Load(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "Features", "Projects",
            "ProjectRunPage.xaml"));
        var bindings = document.Descendants()
            .SelectMany(element => element.Attributes())
            .Select(attribute => attribute.Value)
            .ToArray();

        Assert.Contains(bindings, value => value.Contains("ViewModel.ProjectName", StringComparison.Ordinal));
        Assert.Contains(bindings, value => value.Contains("SelectedTarget.Language", StringComparison.Ordinal));
        Assert.Contains(bindings, value => value.Contains("SelectedTarget.Source", StringComparison.Ordinal));
        Assert.Contains(bindings, value => value.Contains("SelectedTarget.ManifestPath", StringComparison.Ordinal));
        Assert.Contains(bindings, value => value.Contains("SelectedTarget.Entrypoint", StringComparison.Ordinal));
        Assert.Contains(bindings, value => value.Contains("ToggleExpandedCommand", StringComparison.Ordinal));
        Assert.Contains(bindings, value => value.Contains("IsEnvironmentAdvancedExpanded", StringComparison.Ordinal));
        Assert.Contains(bindings, value => value.Contains("IsEnvironmentReady", StringComparison.Ordinal));
    }

    [Fact]
    public void GitWorkbenchProvidesMacRemoteGuidanceAndDiffActions()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var xaml = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "Projects", "ProjectGitPage.xaml"));
        var source = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "Projects", "ProjectGitPage.xaml.cs"));

        Assert.Contains("Click=\"OnPullClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnPushClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnOpenDiffFileClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("ViewModel.CloseDiffCommand", xaml, StringComparison.Ordinal);
        Assert.Contains("if (!ViewModel.HasRemotes)", source, StringComparison.Ordinal);
        Assert.Contains("await ShowRemoteDialogAsync(null);", source, StringComparison.Ordinal);
        Assert.Contains("ProjectFileExternalOpenMode.Default", source, StringComparison.Ordinal);
    }

    [Fact]
    public void AgentTeamWorkspaceMatchesTheMacTeamNavigationAndFullHeightLayout()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var document = XDocument.Load(Path.Combine(
            desktopRoot, "Features", "AgentTeams", "AgentTeamPage.xaml"));
        var pivot = FindElementByAutomationId(document, "ChatOS.AgentTeam.Navigation");
        var headers = pivot.Elements()
            .Where(element => element.Name.LocalName == "PivotItem")
            .Select(element => element.Attribute("Header")?.Value ?? string.Empty)
            .ToArray();

        Assert.Equal(
            ["总览", "聊天", "任务", "需求调研", "共享资产", "运行"],
            headers);
        Assert.Equal("Stretch", pivot.Attribute("HorizontalContentAlignment")?.Value);
        Assert.Equal("Stretch", pivot.Attribute("VerticalContentAlignment")?.Value);
        Assert.Equal(
            "OnOpenRequirementSurveysClick",
            FindElementByAutomationId(document, "ChatOS.AgentTeam.OpenRequirementSurveys")
                .Attribute("Click")?.Value);
        _ = FindElementByAutomationId(document, "ChatOS.AgentTeam.Members");

        var hostSource = File.ReadAllText(Path.Combine(
            desktopRoot, "AppShell", "WorkspaceHostPage.xaml.cs"));
        Assert.Contains("_agentTeamPage.RequirementSurveysRequested", hostSource,
            StringComparison.Ordinal);
        Assert.Contains("SelectWorkspaceTab(\"requirement-surveys\")", hostSource,
            StringComparison.Ordinal);
    }

    [Fact]
    public void AgentWorkspaceMatchesMacManagementAndDocumentNavigation()
    {
        var windowsRoot = FindRepositoryRoot();
        var desktopRoot = Path.Combine(windowsRoot, "src", "ChatOS.Desktop");
        var document = XDocument.Load(Path.Combine(
            desktopRoot, "Features", "AgentTeams", "AgentWorkspacePage.xaml"));

        _ = FindElementByAutomationId(document, "ChatOS.AgentWorkspace.Management");
        _ = FindElementByAutomationId(document, "ChatOS.AgentWorkspace.Documents");
        _ = FindElementByAutomationId(document, "ChatOS.AgentWorkspace.ManagementPage");
        _ = FindElementByAutomationId(document, "ChatOS.AgentWorkspace.DocumentsPage");
        Assert.Contains(document.Descendants(), element =>
            element.Attribute("Text")?.Value == "还没有 Agent");
        Assert.Contains(document.Descendants(), element =>
            element.Attribute("Text")?.Value == "创建后可以直接私聊，也可以加入项目团队。");
        Assert.Contains(document.Descendants(), element =>
            element.Attribute("Content")?.Value == "Skill 管理" &&
            element.Attribute("Click")?.Value == "OnSkillManagementClick");
        Assert.Contains(document.Descendants(), element =>
            element.Attribute("Text")?.Value == "主动巡检  ");
        Assert.Contains(document.Descendants(), element =>
            element.Attribute("Text")?.Value == "还没有本地 Agent 文档");
        Assert.Contains(document.Descendants(), element =>
            element.Attribute("Text")?.Value == "显示当前账户由 Local Agent Host 保存的 Markdown 文档。");
        Assert.Contains(document.Descendants(), element =>
            element.Attribute("Text")?.Value == "正在读取本地文档…");
        Assert.Contains(document.Descendants(), element =>
            element.Attribute("Content")?.Value == "加载更多" &&
            element.Attribute("Click")?.Value == "OnLoadMoreArtifactsClick");

        var pageSource = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "AgentTeams", "AgentWorkspacePage.xaml.cs"));
        Assert.Contains("AgentProfilePermissions.NormalizeStaffPermissions", pageSource,
            StringComparison.Ordinal);
        Assert.Contains("HeartbeatIntervalSeconds", pageSource, StringComparison.Ordinal);
        Assert.Contains("cursor: reset ? null : _artifactNextCursor", pageSource,
            StringComparison.Ordinal);
        Assert.Contains("new MarkdownRichTextBlock { Markdown = markdown }", pageSource,
            StringComparison.Ordinal);

        var clientSource = File.ReadAllText(Path.Combine(
            windowsRoot, "src", "ChatOS.Connector", "LocalAgent",
            "WindowsLocalAgentArtifactClient.cs"));
        Assert.Contains("\"list_artifacts\"", clientSource, StringComparison.Ordinal);
        Assert.Contains("\"get_artifact_data\"", clientSource, StringComparison.Ordinal);
    }

    [Fact]
    public void RequirementSurveyHubKeepsProjectCardsWhenTheOptionalHostIsUnavailable()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var document = XDocument.Load(Path.Combine(
            desktopRoot, "Features", "AgentTeams", "ProjectFeatureHubPage.xaml"));
        var surveyList = FindElementByAutomationId(
            document, "ChatOS.RequirementSurveys.ProjectSummary");
        Assert.Contains(surveyList.Descendants(), element =>
            element.Name.LocalName == "ItemsWrapGrid" &&
            element.Attribute("Orientation")?.Value == "Horizontal");

        var source = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "AgentTeams", "ProjectFeatureHubPage.xaml.cs"));
        Assert.Contains("summaries.Add(new ProjectSurveySummary(project, 0, 0, 0));",
            source, StringComparison.Ordinal);
        Assert.Contains("当前只能显示项目列表", source, StringComparison.Ordinal);

        var projectSource = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "AgentTeams", "ProjectRequirementSurveysPage.xaml.cs"));
        Assert.Contains("项目需求调研暂不可用", projectSource, StringComparison.Ordinal);
    }

    [Fact]
    public void AuthenticationVisualTransitionKeepsTheActiveTitleBarVisibleUntilReplacement()
    {
        var source = File.ReadAllText(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "MainWindow.xaml.cs"));

        AssertAppearsInOrder(
            source,
            "ShellRoot.Visibility = Visibility.Visible;",
            "SetTitleBar(AppTitleBar);",
            "LoginRoot.Visibility = Visibility.Collapsed;");
        AssertAppearsInOrder(
            source,
            "LoginRoot.Visibility = Visibility.Visible;",
            "SetTitleBar(LoginTitleBar);",
            "ShellRoot.Visibility = Visibility.Collapsed;");
    }

    [Fact]
    public void ProjectFilesCanOpenMacStylePetFileWorkbench()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var projectXaml = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "Projects", "ProjectFilesPage.xaml"));
        var projectSource = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "Projects", "ProjectFilesPage.xaml.cs"));
        var workbench = XDocument.Load(Path.Combine(
            desktopRoot, "Features", "Pet", "PetFileWorkbenchWindow.xaml"));

        Assert.Contains("Localization.OpenInPet", projectXaml, StringComparison.Ordinal);
        Assert.Contains("OnOpenInPetClick", projectXaml, StringComparison.Ordinal);
        Assert.Contains("_petWindowController.OpenFileAsync", projectSource, StringComparison.Ordinal);
        _ = FindElementByAutomationId(workbench, "ChatOS.Pet.FileWorkbench");
        _ = FindElementByAutomationId(workbench, "ChatOS.Pet.FileWorkbench.CodePreview");
        _ = FindElementByAutomationId(workbench, "ChatOS.Pet.FileWorkbench.MarkdownPreview");
        _ = FindElementByAutomationId(workbench, "ChatOS.Pet.FileWorkbench.ImagePreview");
    }

    [Fact]
    public void ConversationComposerExposesMacRuntimeControlsAndMarkdownReplies()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var document = XDocument.Load(Path.Combine(
            desktopRoot, "Features", "Chat", "ConversationPage.xaml"));

        _ = FindElementByAutomationId(document, "ChatOS.Chat.ModelPicker");
        _ = FindElementByAutomationId(document, "ChatOS.Chat.RemotePicker");
        _ = FindElementByAutomationId(document, "ChatOS.Chat.Reasoning");
        var composer = FindElementByAutomationId(document, "ChatOS.Chat.Composer");
        var stop = FindElementByAutomationId(document, "ChatOS.Chat.Stop");
        var send = FindElementByAutomationId(document, "ChatOS.Chat.Send");
        Assert.Equal("1", stop.Attribute("Grid.Column")?.Value);
        Assert.Equal("2", send.Attribute("Grid.Column")?.Value);
        Assert.Equal("3", composer.Attribute("Grid.Column")?.Value);
        Assert.Contains(document.Descendants(), element =>
            element.Name.LocalName == "MarkdownRichTextBlock" &&
            element.Attribute("Markdown")?.Value.Contains("Text", StringComparison.Ordinal) == true);
        Assert.Contains(document.Descendants(), element =>
            element.Name.LocalName == "Button" &&
            element.Attribute("Command")?.Value.Contains("ToggleProcessCommand", StringComparison.Ordinal) == true);
        Assert.Equal("34", send.Attribute("Width")?.Value);
        Assert.Equal("17", send.Attribute("CornerRadius")?.Value);
        Assert.Contains(document.Descendants(), element =>
            element.Name.LocalName == "ConversationAttachmentThumbnail" &&
            element.Attribute("Attachment") is not null);
        Assert.Contains(document.Descendants(), element =>
            element.Name.LocalName == "Button" &&
            element.Attribute("Click")?.Value == "OnPreviewAttachmentClicked");
        Assert.Contains(document.Descendants(), element =>
            element.Name.LocalName == "ConversationAttachmentReferenceThumbnail" &&
            element.Attribute("Attachment") is not null);
        Assert.Contains(document.Descendants(), element =>
            element.Name.LocalName == "Button" &&
            element.Attribute("Tag")?.Value == "Process");
        Assert.Contains(document.Descendants(), element =>
            element.Name.LocalName == "Button" &&
            element.Attribute("Tag")?.Value == "Detail");
    }

    [Fact]
    public void TaskGraphWorkspaceShrinksInsideTheConversationHostAndExposesStableRegions()
    {
        var desktopRoot = Path.Combine(FindRepositoryRoot(), "src", "ChatOS.Desktop");
        var document = XDocument.Load(Path.Combine(
            desktopRoot, "Features", "Chat", "ConversationPage.xaml"));
        var source = File.ReadAllText(Path.Combine(
            desktopRoot, "Features", "Chat", "ConversationPage.xaml.cs"));

        var workspace = FindElementByAutomationId(document, "ChatOS.Chat.TaskGraph.Workspace");
        Assert.Null(workspace.Attribute("Width"));
        _ = FindElementByAutomationId(document, "ChatOS.Chat.TaskGraph.CanvasHost");
        var viewport = FindElementByAutomationId(document, "ChatOS.Chat.TaskGraph.CanvasViewport");
        Assert.Equal("Enabled", viewport.Attribute("ZoomMode")?.Value);
        Assert.Equal("0.25", viewport.Attribute("MinZoomFactor")?.Value);
        Assert.Equal("2", viewport.Attribute("MaxZoomFactor")?.Value);
        _ = FindElementByAutomationId(document, "ChatOS.Chat.TaskGraph.Canvas");
        _ = FindElementByAutomationId(document, "ChatOS.Chat.TaskGraph.ZoomOut");
        _ = FindElementByAutomationId(document, "ChatOS.Chat.TaskGraph.ZoomReset");
        _ = FindElementByAutomationId(document, "ChatOS.Chat.TaskGraph.ZoomIn");
        var center = FindElementByAutomationId(document, "ChatOS.Chat.TaskGraph.Center");
        var fit = FindElementByAutomationId(document, "ChatOS.Chat.TaskGraph.Fit");
        Assert.Equal("24", center.Attribute("Width")?.Value);
        Assert.Equal("0", center.Attribute("Padding")?.Value);
        Assert.Equal("24", fit.Attribute("Width")?.Value);
        Assert.Equal("0", fit.Attribute("Padding")?.Value);
        _ = FindElementByAutomationId(document, "ChatOS.Chat.TaskGraph.Inspector");
        _ = FindElementByAutomationId(document, "ChatOS.Chat.TaskGraph.Actions");
        _ = FindElementByAutomationId(document, "ChatOS.Chat.TaskGraph.RequirementPlan");
        _ = FindElementByAutomationId(document, "ChatOS.Chat.TaskGraph.Refresh");
        _ = FindElementByAutomationId(document, "ChatOS.Chat.TaskGraph.Close");
        Assert.Contains("SizeChanged=\"OnConversationPageSizeChanged\"", document.ToString(), StringComparison.Ordinal);
        Assert.Contains("Math.Min(1280, availableWidth)", source, StringComparison.Ordinal);
        Assert.Contains("< 720 => Math.Max(220, workspaceWidth * 0.45)", source, StringComparison.Ordinal);
        Assert.Contains("< 1050 => workspaceWidth * 0.55", source, StringComparison.Ordinal);
        Assert.Contains("Math.Min(760, workspaceWidth * 0.62)", source, StringComparison.Ordinal);
        Assert.Contains("columns.Length * TaskGraphNodeWidth", source, StringComparison.Ordinal);
        Assert.Contains("MessageTaskGraphLayout.ResolveDepths(nodes)", source, StringComparison.Ordinal);
        Assert.Contains("AddTaskGraphEdge(sourcePoint, targetPoint)", source, StringComparison.Ordinal);
        Assert.Contains("CapturePointer(e.Pointer)", source, StringComparison.Ordinal);
        Assert.Contains("PointerPressedEvent", source, StringComparison.Ordinal);
        Assert.Contains("new PointerEventHandler(OnTaskGraphCanvasPointerMoved)", source, StringComparison.Ordinal);
        Assert.Contains("FitTaskGraphCanvas", source, StringComparison.Ordinal);
        Assert.Contains("ChangeView(horizontalOffset, verticalOffset, zoom, true)", source, StringComparison.Ordinal);
    }

    [Fact]
    public void AskUserCardKeepsMacWidthAndExposesStableActions()
    {
        var document = XDocument.Load(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "Features", "Chat", "ConversationPage.xaml"));
        var card = document.Descendants().Single(element => element.Attributes().Any(attribute =>
            attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
            attribute.Value == "{x:Bind PromptAutomationId}"));

        Assert.Equal("760", card.Attribute("Width")?.Value);
        Assert.Equal("760", card.Attribute("MaxWidth")?.Value);
        Assert.Equal("Left", card.Attribute("HorizontalAlignment")?.Value);
        _ = FindElementByAutomationId(document, "ChatOS.Chat.AskUser.SingleChoice");
        _ = FindElementByAutomationId(document, "ChatOS.Chat.AskUser.Cancel");
        _ = FindElementByAutomationId(document, "ChatOS.Chat.AskUser.Submit");
    }

    [Fact]
    public void PetAskUserCardExposesTheSameFormSemanticsAndStableActions()
    {
        var document = XDocument.Load(Path.Combine(
            FindRepositoryRoot(), "src", "ChatOS.Desktop", "Features", "Pet", "PetWindow.xaml"));

        _ = FindElementByAutomationId(document, "ChatOS.Pet.AskUser.Card");
        _ = FindElementByAutomationId(document, "ChatOS.Pet.AskUser.SingleChoice");
        _ = FindElementByAutomationId(document, "ChatOS.Pet.AskUser.Progress");
        _ = FindElementByAutomationId(document, "ChatOS.Pet.AskUser.Cancel");
        _ = FindElementByAutomationId(document, "ChatOS.Pet.AskUser.Submit");

        Assert.Contains(document.Descendants(), element =>
            element.Name.LocalName == "TextBlock" &&
            element.Attribute("Text")?.Value == "{x:Bind ViewModel.ActivePrompt.ChoiceHint, Mode=OneWay}");
        Assert.Contains(document.Descendants(), element =>
            element.Name.LocalName == "CheckBox" &&
            element.Attributes().Any(attribute =>
                attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
                attribute.Value == "{x:Bind PetOptionAutomationId}"));
        Assert.Contains(document.Descendants(), element =>
            element.Name.LocalName == "TextBox" &&
            element.Attribute("MinHeight")?.Value == "{x:Bind InputMinHeight}" &&
            element.Attributes().Any(attribute =>
                attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
                attribute.Value == "{x:Bind PetTextAutomationId}"));
        Assert.Contains(document.Descendants(), element =>
            element.Name.LocalName == "Button" &&
            element.Attribute("Click")?.Value == "OnActivityOpenClicked" &&
            element.Attributes().Any(attribute =>
                attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
                attribute.Value == "{x:Bind ActivityAutomationId}"));
        Assert.DoesNotContain(document.Descendants(), element =>
            element.Name.LocalName == "ListView" &&
            element.Attributes().Any(attribute =>
                attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
                attribute.Value == "ChatOS.Pet.ActivityList"));
    }

    private static void AssertAppearsInOrder(string source, params string[] fragments)
    {
        var position = -1;
        foreach (var fragment in fragments)
        {
            var next = source.IndexOf(fragment, position + 1, StringComparison.Ordinal);
            Assert.True(next > position, $"Expected '{fragment}' after position {position}.");
            position = next;
        }
    }

    private static XElement FindElementByAutomationId(XDocument document, string automationId)
    {
        return document.Descendants().Single(element => element.Attributes().Any(attribute =>
            attribute.Name.LocalName == "AutomationProperties.AutomationId" &&
            attribute.Value == automationId));
    }

    private static string SliceMethod(string source, string startMarker, string endMarker)
    {
        var start = source.IndexOf(startMarker, StringComparison.Ordinal);
        var end = source.IndexOf(endMarker, start + startMarker.Length, StringComparison.Ordinal);
        Assert.True(start >= 0, $"Could not find method marker '{startMarker}'.");
        Assert.True(end > start, $"Could not find method marker '{endMarker}' after '{startMarker}'.");
        return source[start..end];
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
