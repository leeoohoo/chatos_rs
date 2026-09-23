import SwiftUI

enum TerminalLayout {
    static let contentInsets = EdgeInsets(
        top: 8,
        leading: 12,
        bottom: 8,
        trailing: 12
    )
}

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
                NativeTerminalTabLabel(terminal: session.terminal)
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

private struct NativeTerminalTabLabel: View {
    @ObservedObject var terminal: NativeLocalTerminalViewModel

    var body: some View {
        HStack(spacing: 7) {
            Circle()
                .fill(terminal.statusColor)
                .frame(width: 7, height: 7)
            Text(terminal.title)
                .appFont(.subheadline.weight(.medium))
                .lineLimit(1)
        }
    }
}

struct NativeTerminalHeaderView: View {
    @ObservedObject var terminal: NativeLocalTerminalViewModel

    var body: some View {
        HStack {
            Label(terminal.workingDirectory, systemImage: "folder")
                .appFont(.caption.monospaced())
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer()
            StatusCapsule(title: terminal.shellName, color: terminal.statusColor)
            Text(terminal.statusTitle)
                .appFont(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 18)
        .frame(height: 40)
        .background(AppPalette.canvas)
    }
}
