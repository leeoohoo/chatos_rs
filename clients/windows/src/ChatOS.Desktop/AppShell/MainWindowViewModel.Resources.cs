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
        ApplicationResources.Add(CreateApplicationsResource());
        for (var index = 0; index < Contacts.Count; index++)
        {
            var current = Contacts[index];
            var contact = _workspaceSnapshot.Contacts.FirstOrDefault(value => value.Id == current.Id);
            Contacts[index] = contact is null
                ? current with
                {
                    Title = Localization.Text("叽咕狸", "Jiguli"),
                    Subtitle = Localization.Text("和叽咕狸开始对话", "Start a conversation with Jiguli"),
                }
                : current with { Subtitle = ContactSubtitle(contact.Status) };
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
        Localization.InstalledPluginApplications,
        "\uE71D");
}
