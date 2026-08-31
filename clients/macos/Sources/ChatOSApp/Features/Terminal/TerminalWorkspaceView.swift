import SwiftUI

struct TerminalWorkspaceView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var workspace = TerminalWorkspaceViewModel()

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
                TerminalSessionView(terminal: session.terminal)
                    .id(session.id)
            }
        }
        .navigationTitle(
            workspace.selectedSession?.title
                ?? model.localized("终端", english: "Terminal")
        )
        .toolbar { toolbar }
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

                Menu {
                    Button(
                        model.localized("清屏", english: "Clear"),
                        systemImage: "eraser",
                        action: terminal.clear
                    )
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

struct TerminalSessionView: View {
    @ObservedObject var terminal: TerminalViewModel
    var onSubmit: (() -> Void)?
    var showsHeader: Bool
    @FocusState private var commandFocused: Bool

    init(
        terminal: TerminalViewModel,
        onSubmit: (() -> Void)? = nil,
        showsHeader: Bool = true
    ) {
        self.terminal = terminal
        self.onSubmit = onSubmit
        self.showsHeader = showsHeader
    }

    var body: some View {
        VStack(spacing: 0) {
            if showsHeader {
                TerminalHeaderView(terminal: terminal)
                Divider()
            }
            viewport
        }
        .onAppear { restoreCommandFocus() }
        .onChange(of: terminal.focusRequestRevision) { restoreCommandFocus() }
    }

    private var viewport: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 8) {
                    ForEach(terminal.visibleLines) { line in
                        TerminalLineView(line: line, promptPath: promptPath)
                            .id(line.id)
                    }

                    TerminalPromptView(
                        command: $terminal.command,
                        isRunning: terminal.isRunning,
                        promptPath: promptPath,
                        isFocused: $commandFocused,
                        onSubmit: onSubmit ?? terminal.submit
                    )
                    .id("prompt")
                }
                .appFont(.system(size: 13, design: .monospaced))
                .textSelection(.enabled)
                .padding(22)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .background(Color(nsColor: .textBackgroundColor))
            .onChange(of: terminal.lines.count) {
                withAnimation(.easeOut(duration: 0.15)) {
                    proxy.scrollTo("prompt", anchor: .bottom)
                }
            }
        }
    }

    private var promptPath: String {
        URL(fileURLWithPath: terminal.workingDirectory).lastPathComponent
    }

    private func restoreCommandFocus() {
        Task { @MainActor in
            await Task.yield()
            commandFocused = true
        }
    }
}
