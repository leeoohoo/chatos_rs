import SwiftUI

struct TerminalTabsView: View {
    let sessions: [TerminalWorkspaceViewModel.Session]
    let selectedSessionID: UUID?
    let onSelect: (UUID) -> Void
    let onClose: (UUID) -> Void
    let onAdd: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(sessions) { session in
                        terminalTab(session)
                    }

                    Button("新建终端", systemImage: "plus", action: onAdd)
                        .labelStyle(.iconOnly)
                        .buttonStyle(.plain)
                        .help("新建终端")
                }
            }

            Spacer()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .background(AppPalette.surfaceSubtle)
    }

    private func terminalTab(_ session: TerminalWorkspaceViewModel.Session) -> some View {
        HStack(spacing: 7) {
            Button {
                onSelect(session.id)
            } label: {
                HStack(spacing: 7) {
                    Circle().fill(.green).frame(width: 7, height: 7)
                    Text(session.title)
                        .appFont(.subheadline.weight(.medium))
                        .lineLimit(1)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            Button {
                onClose(session.id)
            } label: {
                Image(systemName: "xmark")
                    .appFont(.system(size: 10, weight: .semibold))
                    .frame(width: 18, height: 18)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .help("关闭终端")
        }
        .padding(.leading, 11)
        .padding(.trailing, 7)
        .padding(.vertical, 7)
        .background(tabBackground(for: session.id), in: RoundedRectangle(cornerRadius: 8))
        .overlay {
            RoundedRectangle(cornerRadius: 8)
                .stroke(
                    session.id == selectedSessionID
                        ? Color.accentColor.opacity(0.45)
                        : Color(nsColor: .separatorColor),
                    lineWidth: 1
                )
        }
    }

    private func tabBackground(for id: UUID) -> Color {
        id == selectedSessionID
            ? Color(nsColor: .windowBackgroundColor)
            : Color(nsColor: .controlBackgroundColor).opacity(0.55)
    }
}

struct TerminalHeaderView: View {
    @ObservedObject var terminal: TerminalViewModel

    var body: some View {
        HStack {
            Label(terminal.workingDirectory, systemImage: "folder")
                .appFont(.caption.monospaced())
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer()
            StatusCapsule(title: "zsh", color: .secondary)
        }
        .padding(.horizontal, 18)
        .frame(height: 48)
        .background(AppPalette.canvas)
    }
}

struct TerminalLineView: View {
    let line: TerminalOutputLine
    let promptPath: String

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            if line.kind == .command {
                TerminalPromptPrefix(promptPath: promptPath)
            }
            Text(line.text).foregroundStyle(color)
        }
    }

    private var color: Color {
        switch line.kind {
        case .command, .output: .primary
        case .error: .red
        case .success: AppPalette.terminalGreen
        case .system: .secondary
        }
    }
}

struct TerminalPromptView: View {
    @Binding var command: String
    let isRunning: Bool
    let promptPath: String
    let isFocused: FocusState<Bool>.Binding
    let onSubmit: () -> Void

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            TerminalPromptPrefix(promptPath: promptPath)
            TextField("", text: $command)
                .textFieldStyle(.plain)
                .focused(isFocused)
                .onSubmit(onSubmit)
                .disabled(isRunning)
            if isRunning { ProgressView().controlSize(.small) }
        }
    }
}

private struct TerminalPromptPrefix: View {
    let promptPath: String

    var body: some View {
        Group {
            Text(promptPath).foregroundStyle(Color.accentColor).fontWeight(.semibold)
            Text("%").foregroundStyle(AppPalette.terminalGreen).fontWeight(.bold)
        }
    }
}
