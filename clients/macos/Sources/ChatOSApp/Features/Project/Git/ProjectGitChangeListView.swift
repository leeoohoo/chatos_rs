import ChatOSCore
import SwiftUI

struct ProjectGitChangeListView: View {
    let title: String
    let changes: [ProjectGitChange]
    let staged: Bool
    let isBusy: Bool
    let onOpenDiff: (ProjectGitChange) -> Void
    let onToggleStage: (ProjectGitChange) -> Void
    let onToggleAll: () -> Void

    @State private var isExpanded = true

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 7) {
                Button {
                    isExpanded.toggle()
                } label: {
                    Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                        .appFont(.caption2.weight(.bold))
                        .frame(width: 12)
                }
                .buttonStyle(.plain)
                Text(title)
                    .appFont(.caption.weight(.semibold))
                Text("\(changes.count)")
                    .appFont(.caption2.monospacedDigit())
                    .foregroundStyle(.secondary)
                Spacer()
                Button(action: onToggleAll) {
                    Image(systemName: staged ? "minus" : "plus")
                }
                .buttonStyle(.plain)
                .help(staged ? "全部取消暂存" : "全部暂存")
                .disabled(changes.isEmpty || isBusy)
            }
            .padding(.horizontal, 10)
            .frame(height: 30)
            .background(AppPalette.surfaceSubtle.opacity(0.72))

            if isExpanded {
                ForEach(changes) { change in
                    ProjectGitChangeRow(
                        change: change,
                        staged: staged,
                        isBusy: isBusy,
                        onOpenDiff: { onOpenDiff(change) },
                        onToggleStage: { onToggleStage(change) }
                    )
                    Divider().padding(.leading, 34)
                }
            }
        }
    }
}

private struct ProjectGitChangeRow: View {
    let change: ProjectGitChange
    let staged: Bool
    let isBusy: Bool
    let onOpenDiff: () -> Void
    let onToggleStage: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Button(action: onOpenDiff) {
                HStack(spacing: 8) {
                    Image(systemName: icon)
                        .foregroundStyle(color)
                        .frame(width: 16)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(URL(fileURLWithPath: change.path).lastPathComponent)
                            .appFont(.caption)
                            .lineLimit(1)
                        let parent = URL(fileURLWithPath: change.path).deletingLastPathComponent().path
                        if parent != "." && parent != "/" {
                            Text(parent.hasPrefix("/") ? String(parent.dropFirst()) : parent)
                                .appFont(.caption2.monospaced())
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)
                        }
                    }
                    Spacer(minLength: 4)
                    Text(statusLabel)
                        .appFont(.caption2.weight(.bold))
                        .foregroundStyle(color)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            Button(action: onToggleStage) {
                Image(systemName: staged ? "minus.circle" : "plus.circle")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .help(staged ? "取消暂存" : "暂存")
            .disabled(isBusy)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
    }

    private var icon: String {
        switch change.kind {
        case .added, .untracked: "doc.badge.plus"
        case .deleted: "doc.badge.minus"
        case .renamed: "arrow.right.doc.on.clipboard"
        case .copied: "doc.on.doc"
        case .conflicted: "exclamationmark.triangle.fill"
        case .typeChanged: "arrow.triangle.2.circlepath"
        case .modified: "doc.text"
        }
    }

    private var color: Color {
        switch change.kind {
        case .added, .untracked: .green
        case .deleted: .red
        case .conflicted: .orange
        case .renamed, .copied: .blue
        case .typeChanged, .modified: .yellow
        }
    }

    private var statusLabel: String {
        switch change.kind {
        case .modified: "M"
        case .added: "A"
        case .deleted: "D"
        case .renamed: "R"
        case .copied: "C"
        case .untracked: "U"
        case .conflicted: "!"
        case .typeChanged: "T"
        }
    }
}
