import ChatOSCore
import SwiftUI

struct ProjectGitDiffView: View {
    let diff: ProjectGitDiff
    let onClose: () -> Void
    let onOpenFile: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "plusminus")
                    .foregroundStyle(AppPalette.ai)
                VStack(alignment: .leading, spacing: 2) {
                    Text(URL(fileURLWithPath: diff.path).lastPathComponent)
                        .appFont(.subheadline.weight(.semibold))
                    Text(diff.isStaged ? "暂存区变更" : "工作区变更")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("打开文件", systemImage: "doc.text") { onOpenFile() }
                    .controlSize(.small)
                Button("关闭差异", systemImage: "xmark") { onClose() }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.plain)
            }
            .padding(.horizontal, 16)
            .frame(height: 52)
            .background(.bar)
            Divider()

            if lines.isEmpty {
                ContentUnavailableView(
                    "没有可显示的文本差异",
                    systemImage: "doc.text.magnifyingglass",
                    description: Text("文件可能只有元数据变化，或者内容差异已经不存在。")
                )
                .workspaceFill()
            } else {
                GeometryReader { viewport in
                    ScrollView([.horizontal, .vertical]) {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(lines) { line in
                                ProjectGitDiffLineView(
                                    line: line,
                                    minimumRowWidth: viewport.size.width
                                )
                            }
                        }
                        .padding(.vertical, 8)
                        .frame(
                            minWidth: viewport.size.width,
                            minHeight: viewport.size.height,
                            alignment: .topLeading
                        )
                    }
                    .defaultScrollAnchor(.topLeading)
                }
                .workspaceFill(alignment: .topLeading)
                .background(Color(nsColor: .textBackgroundColor))
            }
        }
        .workspaceFill()
    }

    private var lines: [ProjectGitDiffLine] {
        ProjectGitDiffLine.parse(diff.content)
    }
}

private struct ProjectGitDiffLineView: View {
    let line: ProjectGitDiffLine
    let minimumRowWidth: CGFloat

    var body: some View {
        HStack(spacing: 0) {
            Text(line.oldLine.map(String.init) ?? "")
                .frame(width: 42, alignment: .trailing)
            Text(line.newLine.map(String.init) ?? "")
                .frame(width: 42, alignment: .trailing)
            Text(line.marker)
                .frame(width: 24, alignment: .center)
            Text(line.content.isEmpty ? " " : line.content)
                .fixedSize(horizontal: true, vertical: false)
                .frame(minWidth: max(600, minimumRowWidth - 108), alignment: .leading)
        }
        .frame(minWidth: minimumRowWidth, alignment: .leading)
        .appFont(.system(.caption, design: .monospaced))
        .foregroundStyle(line.foreground)
        .padding(.vertical, 2)
        .background(line.background)
    }
}

private struct ProjectGitDiffLine: Identifiable {
    enum Kind { case header, hunk, context, addition, deletion }

    let id: Int
    let oldLine: Int?
    let newLine: Int?
    let marker: String
    let content: String
    let kind: Kind

    var foreground: Color {
        switch kind {
        case .header, .hunk: AppPalette.ai
        default: .primary
        }
    }

    var background: Color {
        switch kind {
        case .addition: Color.green.opacity(0.13)
        case .deletion: Color.red.opacity(0.12)
        case .hunk: AppPalette.ai.opacity(0.08)
        default: .clear
        }
    }

    static func parse(_ content: String) -> [ProjectGitDiffLine] {
        var oldLine = 0
        var newLine = 0
        return content.split(separator: "\n", omittingEmptySubsequences: false)
            .prefix(8_000)
            .enumerated()
            .map { index, raw in
                let value = String(raw)
                if value.hasPrefix("@@"), let starts = hunkStarts(value) {
                    oldLine = starts.old
                    newLine = starts.new
                    return .init(id: index, oldLine: nil, newLine: nil, marker: "@@", content: value, kind: .hunk)
                }
                if value.hasPrefix("+++") || value.hasPrefix("---")
                    || value.hasPrefix("diff ") || value.hasPrefix("index ") {
                    return .init(id: index, oldLine: nil, newLine: nil, marker: "", content: value, kind: .header)
                }
                if value.hasPrefix("+") {
                    defer { newLine += 1 }
                    return .init(id: index, oldLine: nil, newLine: newLine, marker: "+", content: String(value.dropFirst()), kind: .addition)
                }
                if value.hasPrefix("-") {
                    defer { oldLine += 1 }
                    return .init(id: index, oldLine: oldLine, newLine: nil, marker: "−", content: String(value.dropFirst()), kind: .deletion)
                }
                if value.hasPrefix(" ") {
                    defer { oldLine += 1; newLine += 1 }
                    return .init(id: index, oldLine: oldLine, newLine: newLine, marker: "", content: String(value.dropFirst()), kind: .context)
                }
                return .init(id: index, oldLine: nil, newLine: nil, marker: "", content: value, kind: .header)
            }
    }

    private static func hunkStarts(_ value: String) -> (old: Int, new: Int)? {
        let parts = value.split(separator: " ")
        guard parts.count >= 3 else { return nil }
        func start(_ value: Substring) -> Int? {
            Int(value.dropFirst().split(separator: ",").first ?? "")
        }
        guard let old = start(parts[1]), let new = start(parts[2]) else { return nil }
        return (old, new)
    }
}
