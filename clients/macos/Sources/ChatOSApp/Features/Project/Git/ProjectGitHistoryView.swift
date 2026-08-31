import ChatOSCore
import SwiftUI

struct ProjectGitHistoryView: View {
    let commits: [ProjectGitCommit]
    @State private var isExpanded = true

    var body: some View {
        VStack(spacing: 0) {
            Button {
                isExpanded.toggle()
            } label: {
                HStack(spacing: 7) {
                    Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                        .appFont(.caption2.weight(.bold))
                        .frame(width: 12)
                    Text("提交历史")
                        .appFont(.caption.weight(.semibold))
                    Text("\(commits.count)")
                        .appFont(.caption2.monospacedDigit())
                        .foregroundStyle(.secondary)
                    Spacer()
                }
                .padding(.horizontal, 10)
                .frame(height: 30)
                .contentShape(Rectangle())
                .background(AppPalette.surfaceSubtle.opacity(0.72))
            }
            .buttonStyle(.plain)

            if isExpanded {
                ForEach(Array(commits.enumerated()), id: \.element.id) { index, commit in
                    ProjectGitCommitRow(
                        commit: commit,
                        showsTopLine: index > 0,
                        showsBottomLine: index < commits.count - 1
                    )
                }
            }
        }
    }
}

private struct ProjectGitCommitRow: View {
    let commit: ProjectGitCommit
    let showsTopLine: Bool
    let showsBottomLine: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 9) {
            ZStack(alignment: .top) {
                VStack(spacing: 0) {
                    Rectangle()
                        .fill(showsTopLine ? AppPalette.ai.opacity(0.45) : .clear)
                        .frame(width: 1, height: 10)
                    Circle()
                        .fill(commit.isMerge ? Color.orange : AppPalette.ai)
                        .frame(width: 8, height: 8)
                        .overlay(Circle().stroke(Color.white, lineWidth: 1.5))
                    Rectangle()
                        .fill(showsBottomLine ? AppPalette.ai.opacity(0.45) : .clear)
                        .frame(width: 1, height: 35)
                }
            }
            .frame(width: 12)

            VStack(alignment: .leading, spacing: 4) {
                Text(commit.subject)
                    .appFont(.caption)
                    .lineLimit(2)
                if !visibleDecorations.isEmpty {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 4) {
                            ForEach(visibleDecorations, id: \.self) { decoration in
                                Text(decoration)
                                    .appFont(.caption2.monospaced())
                                    .foregroundStyle(AppPalette.ai)
                                    .padding(.horizontal, 5)
                                    .padding(.vertical, 2)
                                    .background(AppPalette.aiSoft, in: Capsule())
                                    .lineLimit(1)
                            }
                        }
                    }
                }
                HStack(spacing: 5) {
                    Text(commit.shortID)
                        .appFont(.caption2.monospaced())
                    Text(commit.author)
                    if let date = commit.authoredAt {
                        Text(date, style: .relative)
                    }
                }
                .appFont(.caption2)
                .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 10)
        .frame(minHeight: 53, alignment: .top)
    }

    private var visibleDecorations: [String] {
        Array(commit.decorations.compactMap { decoration -> String? in
            if decoration.hasPrefix("chatos/runs/")
                || decoration.hasPrefix("chatos/executions/") {
                return nil
            }
            if decoration.hasPrefix("HEAD -> ") {
                return String(decoration.dropFirst("HEAD -> ".count))
            }
            if decoration.hasPrefix("tag: ") {
                return String(decoration.dropFirst("tag: ".count))
            }
            return decoration
        }.prefix(3))
    }
}
