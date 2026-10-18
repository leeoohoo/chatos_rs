using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Projects;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Remote;
using ChatOS.Connector.Remote;
using ChatOS.Connector.Terminal;

namespace ChatOS.Desktop.AppShell;
public sealed partial class MainWindowViewModel
{
    private int? _availablePluginApplicationCount;

    private void RebuildRemoteResources()
    {
        var selectedId = SelectedResource?.Kind == WorkspaceResourceKind.RemoteConnection
            ? SelectedResource.Id
            : null;
        RemoteResources.Clear();
        foreach (var connection in RemoteConnections.Connections)
        {
            RemoteResources.Add(new ShellResourceViewModel(
                connection.Id,
                WorkspaceResourceKind.RemoteConnection,
                connection.Name,
                $"{connection.Username}@{connection.Host}:{connection.Port}",
                "\uE968"));
        }
        if (selectedId is not null)
        {
            SetSelectedResourceWithoutActivation(
                RemoteResources.FirstOrDefault(value => value.Id == selectedId));
        }
    }

    private void SetSelectedResourceWithoutActivation(ShellResourceViewModel? resource)
    {
        _suppressSelectionActivation = true;
        try
        {
            SelectedResource = resource;
        }
        finally
        {
            _suppressSelectionActivation = false;
        }
    }

    private void RelocalizeResources()
    {
        ApplicationResources.Clear();
        AddWorkspaceTools();
        for (var index = 0; index < Contacts.Count; index++)
        {
            var current = Contacts[index];
            var contact = _workspaceSnapshot.Contacts.FirstOrDefault(value => value.Id == current.Id);
            var conversation = _workspaceSnapshot.Conversations
                .Where(static value => !value.IsArchived && value.ProjectId is null)
                .OrderByDescending(static value => value.UpdatedAt)
                .FirstOrDefault(value =>
                    string.Equals(value.ContactId, current.Id, StringComparison.Ordinal) ||
                    string.Equals(value.ContactAgentId, contact?.AgentId ?? current.Id, StringComparison.Ordinal));
            Contacts[index] = contact is null
                ? current with
                {
                    Title = Localization.Text("叽咕狸", "Jiguli"),
                    Subtitle = conversation?.Title ?? Localization.Text("和叽咕狸开始对话", "Start a conversation with Jiguli"),
                }
                : current with { Subtitle = conversation?.Title ?? ContactSubtitle(contact.Status) };
        }

        for (var index = 0; index < Projects.Count; index++)
        {
            var current = Projects[index];
            var project = _workspaceSnapshot.Projects.FirstOrDefault(value => value.Id == current.Id);
            if (project is not null && string.IsNullOrWhiteSpace(project.DisplayRootPath ?? project.RootPath))
            {
                Projects[index] = current with { Subtitle = Localization.Projects };
            }
        }

        for (var index = 0; index < LocalResources.Count; index++)
        {
            var current = LocalResources[index];
            if (current.Kind != WorkspaceResourceKind.LocalTerminal) continue;
            if (current.Id == BuiltInLocalTerminalId)
            {
                LocalResources[index] = current with
                {
                    Title = Localization.Text("本机终端", "Local Terminal"),
                    Subtitle = Localization.Text("可用", "Available"),
                };
                continue;
            }
            var workspace = LocalConnectorStatus?.Workspaces.FirstOrDefault(value => value.Id == current.WorkspaceId);
            var alias = workspace?.Alias ?? current.Title.Split('·', 2)[0].Trim();
            LocalResources[index] = current with { Title = $"{alias} · {Localization.Terminal}" };
        }

        if (LocalConnectorStatus is not null)
        {
            ApplyLocalConnectorStatus(LocalConnectorStatus);
        }

        if (SelectedResource is { } selected)
        {
            SetSelectedResourceWithoutActivation(Contacts.Concat(Projects).Concat(ApplicationResources).Concat(LocalResources).Concat(RemoteResources)
                .FirstOrDefault(value => value.Kind == selected.Kind && value.Id == selected.Id));
        }
    }

    private ShellResourceViewModel CreateApplicationsResource() => new(
        "applications",
        WorkspaceResourceKind.Applications,
        Localization.Applications,
        _availablePluginApplicationCount is > 0
            ? Localization.Text(
                $"{_availablePluginApplicationCount} 个可用",
                $"{_availablePluginApplicationCount} available")
            : Localization.InstalledPluginApplications,
        "\uE71D");

    public void UpdatePluginApplicationCount(int? count)
    {
        _availablePluginApplicationCount = count is null ? null : Math.Max(0, count.Value);
        var index = -1;
        for (var candidate = 0; candidate < ApplicationResources.Count; candidate++)
        {
            if (ApplicationResources[candidate].Kind != WorkspaceResourceKind.Applications) continue;
            index = candidate;
            break;
        }
        if (index < 0) return;

        var updated = CreateApplicationsResource();
        ApplicationResources[index] = updated;
        if (SelectedResource?.Kind == WorkspaceResourceKind.Applications)
        {
            SetSelectedResourceWithoutActivation(updated);
        }
    }

    private void AddWorkspaceTools()
    {
        ApplicationResources.Add(CreateApplicationsResource());
        ApplicationResources.Add(new ShellResourceViewModel(
            "media-studio",
            WorkspaceResourceKind.MediaStudio,
            Localization.Text("AI 创作", "AI Creation"),
            Localization.Text("生成图片与视频", "Generate images and video"),
            "\uE735"));
        ApplicationResources.Add(new ShellResourceViewModel(
            "agent-teams",
            WorkspaceResourceKind.AgentTeams,
            "Agent",
            Localization.Text("私聊与项目团队", "Direct messages and project teams"),
            "\uE716"));
        ApplicationResources.Add(new ShellResourceViewModel(
            "requirement-surveys",
            WorkspaceResourceKind.RequirementSurveys,
            Localization.Text("需求调研", "Requirement Surveys"),
            Localization.Text("需求确认、方案与执行计划", "Requirements, solutions, and execution plans"),
            "\uE9D5"));
    }
}
