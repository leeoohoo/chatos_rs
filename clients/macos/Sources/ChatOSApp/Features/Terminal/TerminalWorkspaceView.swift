import SwiftUI

struct TerminalWorkspaceView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject private var workspace: TerminalWorkspaceViewModel

    init(workspace: TerminalWorkspaceViewModel) {
        self.workspace = workspace
    }

    var body: some View {
        VStack(spacing: 0) {
            TerminalTabsView(
                sessions: workspace.sessions,
                selectedSessionID: workspace.selectedSessionID,
                onSelect: workspace.selectTerminal,
                onClose: workspace.closeTerminal,
                onAdd: workspace.createTerminal
            )
            Divider()
            if let session = workspace.selectedSession {
                NativeLocalTerminalSessionView(terminal: session.terminal)
                    .id(session.id)
            }
        }
        .navigationTitle(
            workspace.selectedSession?.title
                ?? model.localized("终端", english: "Terminal")
        )
        .toolbar { toolbar }
        .onAppear(perform: workspace.ensureTerminal)
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .primaryAction) {
            if let terminal = workspace.selectedSession?.terminal {
                TextField(
                    model.localized("搜索终端输出", english: "Search terminal output"),
                    text: Binding(
                        get: { terminal.searchText },
                        set: { terminal.searchText = $0 }
                    )
                )
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 190)

                Button(
                    model.localized("上一个匹配项", english: "Previous Match"),
                    systemImage: "chevron.up",
                    action: terminal.findPrevious
                )
                .labelStyle(.iconOnly)
                .disabled(terminal.searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)

                Button(
                    model.localized("下一个匹配项", english: "Next Match"),
                    systemImage: "chevron.down",
                    action: terminal.findNext
                )
                .labelStyle(.iconOnly)
                .disabled(terminal.searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)

                Menu {
                    Button(
                        model.localized("清屏", english: "Clear"),
                        systemImage: "eraser",
                        action: terminal.clear
                    )
                    Divider()
                    Button(
                        model.localized("中断", english: "Interrupt"),
                        systemImage: "stop.fill",
                        action: terminal.interrupt
                    )
                    .disabled(terminal.state != .running)
                    Divider()
                    Button(
                        model.localized("关闭终端", english: "Close Terminal"),
                        systemImage: "xmark",
                        action: workspace.closeSelectedTerminal
                    )
                } label: {
                    Image(systemName: "ellipsis.circle")
                }
            }
        }
    }
}

struct NativeLocalTerminalSessionView: View {
    @ObservedObject var terminal: NativeLocalTerminalViewModel

    var body: some View {
        VStack(spacing: 0) {
            NativeTerminalHeaderView(terminal: terminal)
            Divider()
            NativeLocalTerminalSurface(terminal: terminal)
                .padding(TerminalLayout.contentInsets)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color(nsColor: .textBackgroundColor))

            if case let .failed(message) = terminal.state {
                HStack(spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                    Text(message).lineLimit(2)
                    Spacer()
                }
                .appFont(.caption)
                .foregroundStyle(.red)
                .padding(.horizontal, 14)
                .frame(minHeight: 30)
                .background(Color.red.opacity(0.08))
            }
        }
        .onAppear { terminal.focus() }
    }
}
