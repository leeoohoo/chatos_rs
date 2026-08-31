import ChatOSCore
import SwiftUI

struct SFTPBrowserHeader: View {
    let title: String
    let path: String
    let canGoUp: Bool
    let isLoading: Bool
    let onChooseLocation: (() -> Void)?
    let onGoUp: () -> Void
    let onRefresh: () -> Void
    let trailingActions: AnyView?

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Text(title).appFont(.headline)
                Spacer()
                if let trailingActions { trailingActions }
                if let onChooseLocation {
                    Button("选择目录", systemImage: "folder.badge.gearshape", action: onChooseLocation)
                        .labelStyle(.iconOnly)
                        .help("选择目录")
                }
                Button("上级目录", systemImage: "arrow.up", action: onGoUp)
                    .labelStyle(.iconOnly)
                    .disabled(!canGoUp)
                    .help("上级目录")
                Button("刷新", systemImage: "arrow.clockwise", action: onRefresh)
                    .labelStyle(.iconOnly)
                    .disabled(isLoading)
                    .help("刷新")
            }
            .buttonStyle(.borderless)
            .padding(.horizontal, 12)
            .frame(height: 40)

            HStack(spacing: 7) {
                Image(systemName: "folder")
                    .foregroundStyle(Color.accentColor)
                Text(path)
                    .appFont(.caption.monospaced())
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
                Spacer()
                if isLoading { ProgressView().controlSize(.small) }
            }
            .padding(.horizontal, 12)
            .frame(height: 34)
            .background(AppPalette.surfaceSubtle)
        }
    }
}

struct SFTPColumnHeader: View {
    var body: some View {
        HStack(spacing: 10) {
            Text("名称").frame(maxWidth: .infinity, alignment: .leading)
            Text("修改时间").frame(width: 126, alignment: .leading)
            Text("大小").frame(width: 72, alignment: .trailing)
        }
        .appFont(.caption2.weight(.semibold))
        .foregroundStyle(.secondary)
        .padding(.horizontal, 12)
        .frame(height: 28)
        .background(AppPalette.surfaceSubtle)
    }
}

struct SFTPFileRow: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.locale) private var locale
    let name: String
    let isDirectory: Bool
    let size: Int64?
    let modifiedAt: Date?
    let isSelected: Bool
    let onOpen: () -> Void
    let onSelect: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            HStack(spacing: 8) {
                Image(systemName: isDirectory ? "folder.fill" : fileIcon)
                    .foregroundStyle(isDirectory ? Color.accentColor : .secondary)
                    .frame(width: 18)
                Text(name)
                    .appFont(.body)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            Text(modifiedAt.map(formatDate) ?? "—")
                .frame(width: 126, alignment: .leading)
            Text(size.map(formatBytes) ?? "—")
                .frame(width: 72, alignment: .trailing)
        }
        .appFont(.caption)
        .padding(.horizontal, 12)
        .frame(height: 34)
        .contentShape(Rectangle())
        .background(isSelected ? Color.accentColor.opacity(0.14) : .clear)
        .onTapGesture(count: 2, perform: onOpen)
        .onTapGesture(perform: onSelect)
    }

    private var fileIcon: String {
        let ext = (name as NSString).pathExtension.lowercased()
        if ["png", "jpg", "jpeg", "gif", "webp", "heic"].contains(ext) { return "photo" }
        if ["zip", "gz", "tgz", "tar", "7z"].contains(ext) { return "archivebox" }
        if ["swift", "rs", "js", "ts", "py", "go", "java", "c", "cpp", "h"].contains(ext) {
            return "chevron.left.forwardslash.chevron.right"
        }
        return "doc"
    }

    private func formatDate(_ value: Date) -> String {
        value.formatted(
            .dateTime
                .year(.twoDigits)
                .month(.twoDigits)
                .day(.twoDigits)
                .hour()
                .minute()
                .locale(locale)
        )
    }

    private func formatBytes(_ value: Int64) -> String {
        guard value < 1_024 else {
            return ByteCountFormatter.string(fromByteCount: value, countStyle: .file)
        }
        return model.localized("\(value) 字节", english: "\(value) bytes")
    }
}

struct SFTPEmptyDirectoryView: View {
    let isLoading: Bool

    var body: some View {
        Group {
            if isLoading {
                ProgressView("正在读取目录…")
            } else {
                ContentUnavailableView("空目录", systemImage: "folder")
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
