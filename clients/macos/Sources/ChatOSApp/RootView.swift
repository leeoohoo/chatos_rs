import SwiftUI

struct RootView: View {
    @EnvironmentObject private var model: AppModel
    @State private var showingNotepad = false

    var body: some View {
        AuthenticationGateView(authentication: model.authentication) {
            workspace
        }
    }

    private var workspace: some View {
        NavigationSplitView {
            ResourceSidebar()
                .navigationSplitViewColumnWidth(min: 220, ideal: 244, max: 290)
        } detail: {
            ZStack(alignment: .topTrailing) {
                detail
                    .workspaceFill()

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
        .tint(.accentColor)
        .toolbar { WorkspaceToolbar(showingNotepad: $showingNotepad) }
        .sheet(isPresented: $showingNotepad) {
            NotepadSheet(service: model.notepadService) {
                showingNotepad = false
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
            case .terminal:
                TerminalWorkspaceView()
            case let .remote(remoteID):
                RemoteConnectionDetailView(connectionID: remoteID)
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

private struct WorkspaceToolbar: ToolbarContent {
    @EnvironmentObject private var model: AppModel
    @Binding var showingNotepad: Bool

    var body: some ToolbarContent {
        ToolbarItemGroup(placement: .primaryAction) {
            Button { showingNotepad = true } label: {
                Image(systemName: "note.text")
            }
            .help(model.localized("打开记事本", english: "Open Notepad"))

            Menu {
                SettingsLink {
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
            }
            .help(model.localized("账号", english: "Account"))
        }
    }
}
