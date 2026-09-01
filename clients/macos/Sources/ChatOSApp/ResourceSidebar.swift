import SwiftUI

struct ResourceSidebar: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.interfaceFontScale) private var interfaceFontScale
    @State private var creationSheet: ResourceCreationSheet?
    @State private var projectDeletionAlert: SidebarProjectDeletionAlert?

    var body: some View {
        List(selection: $model.selection) {
            Section {
                if model.isWorkspaceLoading && model.contacts.isEmpty {
                    loadingRow(model.localized("正在加载联系人…", english: "Loading contacts…"))
                }
                ForEach(model.contacts) { contact in
                    resourceRow(
                        title: contact.title,
                        subtitle: contact.subtitle,
                        systemImage: "person.crop.circle.fill",
                        tint: .secondary
                    )
                    .tag(SidebarSelection.contact(contact.id))
                }
            } header: {
                sectionHeader(model.localized("联系人", english: "Contacts"))
            }

            Section {
                if model.isWorkspaceLoading && model.projects.isEmpty {
                    loadingRow(model.localized("正在加载项目…", english: "Loading projects…"))
                }
                ForEach(model.projects) { project in
                    resourceRow(
                        title: project.title,
                        subtitle: project.subtitle,
                        systemImage: "folder",
                        tint: .accentColor
                    )
                    .tag(SidebarSelection.project(project.id))
                    .contextMenu {
                        Button(
                            model.localized("删除项目", english: "Delete Project"),
                            systemImage: "trash",
                            role: .destructive
                        ) {
                            projectDeletionAlert = .confirmation(project)
                        }
                    }
                }
            } header: {
                sectionHeader(model.localized("项目", english: "Projects"))
            }

            Section {
                ForEach(model.terminals) { terminal in
                    resourceRow(
                        title: terminal.title,
                        subtitle: terminal.subtitle,
                        systemImage: "terminal",
                        tint: AppPalette.terminalGreen
                    )
                    .tag(SidebarSelection.terminal(terminal.id))
                }
            } header: {
                sectionHeader(model.localized("本机", english: "Local"))
            }

            Section {
                if model.isRemoteConnectionsLoading && model.remoteConnections.isEmpty {
                    loadingRow(model.localized(
                        "正在加载远端连接…",
                        english: "Loading remote connections…"
                    ))
                } else if model.remoteConnections.isEmpty {
                    HStack(spacing: 10) {
                        Image(systemName: "network")
                            .frame(width: 18)
                        Text(model.localized("还没有远端连接", english: "No remote connections"))
                            .appFont(.body)
                    }
                    .foregroundStyle(.secondary)
                    .padding(.vertical, rowVerticalPadding)
                } else {
                    ForEach(model.remoteConnections) { connection in
                        resourceRow(
                            title: connection.name,
                            subtitle: "\(connection.username)@\(connection.host):\(connection.port)",
                            systemImage: "network",
                            tint: .accentColor
                        )
                        .tag(SidebarSelection.remote(connection.id))
                        .contextMenu {
                            Button(model.localized("编辑", english: "Edit"), systemImage: "pencil") {
                                creationSheet = .editRemoteConnection(connection.id)
                            }
                        }
                    }
                }
            } header: {
                sectionHeader(model.localized("远端", english: "Remote"))
            }

            if let remoteError = model.remoteConnectionsError {
                Section {
                    VStack(alignment: .leading, spacing: 7) {
                        Label(
                            model.localized(
                                "远端连接加载失败",
                                english: "Failed to load remote connections"
                            ),
                            systemImage: "exclamationmark.triangle.fill"
                        )
                            .foregroundStyle(.orange)
                        Text(remoteError).appFont(.caption2).foregroundStyle(.secondary)
                        Button(
                            model.localized("重试", english: "Retry"),
                            action: model.refreshRemoteConnections
                        )
                        .controlSize(.small)
                    }
                    .padding(.vertical, 4)
                }
            }

            if let workspaceError = model.workspaceError {
                Section {
                    VStack(alignment: .leading, spacing: 7) {
                        Label(
                            model.localized("资源同步失败", english: "Resource sync failed"),
                            systemImage: "exclamationmark.triangle.fill"
                        )
                            .foregroundStyle(.orange)
                        Text(workspaceError)
                            .appFont(.caption2)
                            .foregroundStyle(.secondary)
                        Button(model.localized("重试", english: "Retry"), action: model.refreshWorkspace)
                            .controlSize(.small)
                    }
                    .padding(.vertical, 4)
                }
            }
        }
        .listStyle(.sidebar)
        .navigationTitle("ChatOS")
        .toolbar {
            ToolbarItem(placement: .automatic) {
                Button(
                    model.localized("刷新资源", english: "Refresh Resources"),
                    systemImage: "arrow.clockwise",
                    action: model.refreshAllResources
                )
                    .labelStyle(.iconOnly)
                    .disabled(model.isWorkspaceLoading)
            }
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    Button(model.localized("新建项目", english: "New Project"), systemImage: "folder.badge.plus") {
                        creationSheet = .project
                    }
                    Divider()
                    Button(
                        model.localized("新建远端连接", english: "New Remote Connection"),
                        systemImage: "network.badge.shield.half.filled"
                    ) {
                        creationSheet = .createRemoteConnection
                    }
                } label: {
                    Image(systemName: "plus")
                }
                .help(model.localized("新建资源", english: "New Resource"))
            }
        }
        .sheet(item: $creationSheet) { sheet in
            switch sheet {
            case .project:
                CreateProjectSheetHost(
                    connectorStatus: model.localConnectorControl.status,
                    defaultContact: model.defaultProjectContact,
                    filesystemService: model.projectFilesystemService,
                    gitService: model.projectGitService,
                    creationService: model.workspaceResourceCreationService,
                    onCreated: model.registerCreatedProject
                )
            case .createRemoteConnection:
                RemoteConnectionEditorSheetHost(
                    editingConnection: nil,
                    connections: model.remoteConnections,
                    service: model.remoteConnectionService,
                    onSaved: model.registerRemoteConnection
                )
            case let .editRemoteConnection(id):
                RemoteConnectionEditorSheetHost(
                    editingConnection: model.remoteConnection(id: id),
                    connections: model.remoteConnections,
                    service: model.remoteConnectionService,
                    onSaved: model.registerRemoteConnection
                )
            }
        }
        .alert(item: $projectDeletionAlert) { alert in
            switch alert {
            case let .confirmation(project):
                Alert(
                    title: Text(model.localized("删除项目？", english: "Delete Project?")),
                    message: Text(model.localized(
                        "“\(project.title)”会从 ChatOS 中移除，本机项目文件夹不会被删除。",
                        english: "\(project.title) will be removed from ChatOS. Its local folder will not be deleted."
                    )),
                    primaryButton: .destructive(Text(model.localized("删除", english: "Delete"))) {
                        Task { await deleteProject(project) }
                    },
                    secondaryButton: .cancel(Text(model.localized("取消", english: "Cancel")))
                )
            case let .failure(message):
                Alert(
                    title: Text(model.localized("项目删除失败", english: "Project Deletion Failed")),
                    message: Text(message),
                    dismissButton: .default(Text(model.localized("好", english: "OK")))
                )
            }
        }
    }

    private func resourceRow(
        title: String,
        subtitle: String?,
        systemImage: String,
        tint: Color
    ) -> some View {
        HStack(spacing: 10) {
            Image(systemName: systemImage)
                .foregroundStyle(tint)
                .frame(width: 18)

            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .appFont(.body.weight(.medium))
                    .lineLimit(1)
                if let subtitle {
                    Text(subtitle)
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
        }
        .padding(.vertical, rowVerticalPadding)
    }

    private var rowVerticalPadding: CGFloat {
        3 + max(0, interfaceFontScale - 1) * 5
    }

    private func sectionHeader(_ title: String) -> some View {
        Text(title)
            .appFont(.caption.weight(.semibold))
    }

    private func loadingRow(_ title: String) -> some View {
        HStack(spacing: 9) {
            ProgressView()
                .controlSize(.small)
            Text(title)
                .appFont(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.vertical, rowVerticalPadding)
    }

    private func deleteProject(_ project: ResourceItem) async {
        do {
            try await model.deleteProject(id: project.id)
        } catch {
            projectDeletionAlert = .failure(error.localizedDescription)
        }
    }
}

private enum SidebarProjectDeletionAlert: Identifiable {
    case confirmation(ResourceItem)
    case failure(String)

    var id: String {
        switch self {
        case let .confirmation(project): "confirmation-\(project.id)"
        case let .failure(message): "failure-\(message)"
        }
    }
}

private enum ResourceCreationSheet: Identifiable {
    case project
    case createRemoteConnection
    case editRemoteConnection(String)

    var id: String {
        switch self {
        case .project: "project"
        case .createRemoteConnection: "remote-create"
        case let .editRemoteConnection(id): "remote-edit-\(id)"
        }
    }
}
