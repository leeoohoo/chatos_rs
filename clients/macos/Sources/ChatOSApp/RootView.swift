import ChatOSConnector
import SwiftUI

struct RootView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        AuthenticationGateView(authentication: model.authentication) {
            workspace
        }
    }

    private var workspace: some View {
        NavigationSplitView(columnVisibility: $model.navigationSplitVisibility) {
            ResourceSidebar()
                .navigationSplitViewColumnWidth(min: 220, ideal: 244, max: 290)
        } detail: {
            ZStack(alignment: .topTrailing) {
                detail
                    .workspaceFill()

                LocalAgentHostStatusView(
                    state: model.localAgentHostState,
                    error: model.localAgentHostError
                )
                .padding(.top, 12)
                .frame(maxWidth: .infinity, alignment: .top)
                .zIndex(40)

                GlobalApprovalOverlayHost(viewModel: model.localConnectorControl)
                    .padding(18)
                    .zIndex(30)

                VisualSessionOverlayHost(
                    store: model.visualSessionStore,
                    currentConversationID: model.currentConversationID
                )
                .padding(18)
                .frame(
                    maxWidth: .infinity,
                    maxHeight: .infinity,
                    alignment: model.localConnectorControl.pendingApprovals.isEmpty
                        ? .topTrailing
                        : .bottomTrailing
                )
                .zIndex(20)
            }
            .workspaceFill()
        }
        .navigationSplitViewStyle(.balanced)
        .toolbar(removing: .sidebarToggle)
        .tint(.accentColor)
        .sheet(isPresented: $model.isNotepadPresented) {
            NotepadSheet(service: model.notepadService) {
                model.isNotepadPresented = false
            }
        }
    }

    @ViewBuilder
    private var detail: some View {
        Group {
            switch model.selection {
            case let .project(projectID):
                ProjectWorkspaceView(projectID: projectID)
                    .id(projectID)
            case let .contact(contactID):
                ContactConversationView(contactID: contactID)
            case .localConnector:
                LocalConnectorControlCenterView(viewModel: model.localConnectorControl)
            case .applications:
                PluginApplicationsView()
            case .mediaStudio:
                MediaStudioView(viewModel: model.mediaStudio)
            case let .pluginApplication(pluginID, componentKey):
                if let application = model.pluginApplication(
                    pluginID: pluginID,
                    componentKey: componentKey
                ) {
                    PluginApplicationHostView(application: application)
                        .id(application.id)
                } else {
                    PluginApplicationsView()
                }
            case .terminal:
                TerminalWorkspaceView()
            case let .remote(remoteID):
                RemoteConnectionDetailView(connectionID: remoteID)
                    .id(remoteID)
            case nil:
                ContentUnavailableView(
                    model.localized("选择一个资源开始", english: "Select a resource to begin"),
                    systemImage: "sidebar.left",
                    description: Text(model.localized(
                        "联系人用于持续对话，项目包含目录、用户消息、Plan 和运行设置。",
                        english: "Contacts provide ongoing conversations. Projects include files, messages, plans, and runtime settings."
                    ))
                )
            }
        }
        .workspaceFill()
    }
}

private struct LocalAgentHostStatusView: View {
    @EnvironmentObject private var model: AppModel
    let state: NativeLocalAgentHostState
    let error: String?

    @ViewBuilder
    var body: some View {
        switch state {
        case .running:
            EmptyView()
        case .starting:
            status(
                model.localized("正在启动本地 Agent…", english: "Starting Local Agent…"),
                systemImage: "arrow.triangle.2.circlepath",
                color: .secondary,
                showsProgress: true
            )
        case let .restarting(_, attempt):
            status(
                model.localized(
                    "本地 Agent 正在恢复（第 \(attempt) 次）…",
                    english: "Recovering Local Agent (attempt \(attempt))…"
                ),
                systemImage: "exclamationmark.arrow.triangle.2.circlepath",
                color: .orange,
                showsProgress: true
            )
        case let .failed(_, reason):
            status(
                error ?? reason,
                systemImage: "exclamationmark.triangle.fill",
                color: .red,
                showsProgress: false
            )
        case .stopped:
            if let error {
                status(
                    error,
                    systemImage: "stop.circle.fill",
                    color: .red,
                    showsProgress: false
                )
            }
        }
    }

    private func status(
        _ text: String,
        systemImage: String,
        color: Color,
        showsProgress: Bool
    ) -> some View {
        HStack(spacing: 8) {
            if showsProgress {
                ProgressView().controlSize(.small)
            } else {
                Image(systemName: systemImage)
            }
            Text(text)
                .appFont(.caption.weight(.medium))
                .lineLimit(2)
        }
        .foregroundStyle(color)
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.regularMaterial, in: Capsule())
        .overlay(Capsule().stroke(color.opacity(0.24), lineWidth: 1))
        .shadow(color: .black.opacity(0.08), radius: 10, y: 4)
        .padding(.horizontal, 24)
        .accessibilityLabel(text)
    }
}

struct WorkspaceTitlebarActionsView: View {
    @ObservedObject var model: AppModel
    @ObservedObject private var authentication: AuthenticationViewModel

    init(model: AppModel) {
        self.model = model
        _authentication = ObservedObject(wrappedValue: model.authentication)
    }

    var body: some View {
        Group {
            if case .authenticated = authentication.phase {
                controls
            }
        }
        .padding(.horizontal, 8)
        .frame(height: 36)
    }

    private var controls: some View {
        HStack(spacing: 8) {
            Button { model.isNotepadPresented = true } label: {
                Image(systemName: "note.text")
                    .frame(width: 28, height: 28)
                    .background(Color(nsColor: .controlBackgroundColor), in: Circle())
            }
            .buttonStyle(.plain)
            .help(model.localized("打开记事本", english: "Open Notepad"))

            Menu {
                Button {
                    model.openGlobalSearchSettings()
                } label: {
                    Label(model.localized("设置", english: "Settings"), systemImage: "gear")
                }
                Divider()
                Button(
                    model.localized("退出登录", english: "Sign Out"),
                    systemImage: "rectangle.portrait.and.arrow.right"
                ) {
                    model.authentication.logout()
                }
            } label: {
                Image(systemName: "person.crop.circle")
                    .frame(width: 28, height: 28)
                    .background(Color(nsColor: .controlBackgroundColor), in: Circle())
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.visible)
            .help(model.localized("账号", english: "Account"))
        }
    }
}
