import ChatOSCore
import SwiftUI

struct ClipboardHistoryView: View {
    @ObservedObject var viewModel: ClipboardHistoryViewModel
    let isEnglish: Bool

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().opacity(0.65)
            content
            Divider().opacity(0.65)
            footer
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.ultraThickMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .strokeBorder(.white.opacity(0.16), lineWidth: 1)
        }
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private var header: some View {
        HStack(spacing: 14) {
            Image(systemName: "clipboard")
                .font(.system(size: 21, weight: .semibold))
                .foregroundStyle(.secondary)
            GlobalCommandSearchField(
                text: Binding(
                    get: { viewModel.query },
                    set: { viewModel.updateQuery($0) }
                ),
                placeholder: isEnglish ? "Search clipboard history" : "搜索剪贴板历史",
                fontSize: 20,
                onMove: viewModel.moveSelection,
                onSubmit: viewModel.restoreSelected,
                onCancel: viewModel.cancel
            )
            if viewModel.isLoading {
                ProgressView().controlSize(.small)
            }
            Button {
                viewModel.clearHistory()
            } label: {
                Image(systemName: "trash")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .disabled(viewModel.entries.isEmpty)
            .help(isEnglish ? "Clear history" : "清空历史")
        }
        .padding(.horizontal, 20)
        .frame(height: 68)
    }

    @ViewBuilder
    private var content: some View {
        if viewModel.isLoading, viewModel.entries.isEmpty {
            ProgressView(isEnglish ? "Loading clipboard history…" : "正在读取剪贴板历史…")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if let error = viewModel.errorMessage {
            ContentUnavailableView(
                isEnglish ? "Clipboard History Error" : "剪贴板历史异常",
                systemImage: "exclamationmark.triangle",
                description: Text(error)
            )
        } else if viewModel.filteredEntries.isEmpty {
            ClipboardHistoryEmptyState(
                hasStoredEntries: !viewModel.entries.isEmpty,
                isEnglish: isEnglish
            )
        } else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 4) {
                        ForEach(Array(viewModel.filteredEntries.enumerated()), id: \.element.id) { index, entry in
                            ClipboardHistoryRow(
                                entry: entry,
                                sourceName: viewModel.sourceName(for: entry),
                                thumbnail: viewModel.thumbnail(for: entry),
                                isSelected: viewModel.selectedIndex == index,
                                isEnglish: isEnglish
                            )
                            .id(entry.id)
                            .contentShape(Rectangle())
                            .onAppear { viewModel.loadThumbnailIfNeeded(for: entry) }
                            .onTapGesture {
                                viewModel.select(index)
                                viewModel.restore(entry)
                            }
                        }
                    }
                    .padding(8)
                }
                .onChange(of: viewModel.selectedIndex) { _, index in
                    guard viewModel.filteredEntries.indices.contains(index) else { return }
                    proxy.scrollTo(viewModel.filteredEntries[index].id, anchor: .center)
                }
            }
        }
    }

    private var footer: some View {
        HStack(spacing: 16) {
            if let noticeMessage = viewModel.noticeMessage {
                Text(isEnglish
                    ? "Copied. Grant Accessibility access, then choose it again to paste automatically."
                    : noticeMessage)
                    .foregroundStyle(.orange)
                    .lineLimit(1)
            } else {
                Text(isEnglish ? "Stored only on this Mac" : "仅保存在这台 Mac 上")
            }
            Spacer()
            if viewModel.filteredEntries.isEmpty {
                hint("esc", isEnglish ? "Close" : "关闭")
            } else {
                hint("⌘P", isEnglish ? "Pin" : "固定")
                    .onTapGesture(perform: viewModel.togglePinSelected)
                hint("⌫", isEnglish ? "Delete" : "删除")
                    .onTapGesture(perform: viewModel.deleteSelected)
                hint("↩", isEnglish ? "Paste" : "粘贴")
            }
        }
        .font(.system(size: 11, weight: .medium))
        .foregroundStyle(.secondary)
        .padding(.horizontal, 16)
        .frame(height: 38)
    }

    private func hint(_ key: String, _ label: String) -> some View {
        HStack(spacing: 5) {
            Text(key)
                .font(.system(size: 10, weight: .bold, design: .rounded))
                .padding(.horizontal, 5)
                .padding(.vertical, 2)
                .background(.quaternary, in: RoundedRectangle(cornerRadius: 4))
            Text(label)
        }
    }
}

private struct ClipboardHistoryEmptyState: View {
    let hasStoredEntries: Bool
    let isEnglish: Bool

    var body: some View {
        VStack(spacing: 13) {
            Image(systemName: hasStoredEntries ? "magnifyingglass" : "clipboard")
                .font(.system(size: 24, weight: .semibold))
                .foregroundStyle(Color.accentColor)
                .frame(width: 52, height: 52)
                .background(Color.accentColor.opacity(0.11), in: RoundedRectangle(cornerRadius: 14))
            Text(title)
                .font(.system(size: 15, weight: .semibold))
            Text(detail)
                .font(.system(size: 12.5))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 390)
        }
        .padding(.horizontal, 28)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var title: String {
        if hasStoredEntries {
            return isEnglish ? "No Matching Clipboard Items" : "没有匹配的剪贴板内容"
        }
        return isEnglish ? "Clipboard History Is Empty" : "剪贴板历史还是空的"
    }

    private var detail: String {
        if hasStoredEntries {
            return isEnglish ? "Try another keyword." : "换一个关键词再试试。"
        }
        return isEnglish
            ? "Copy text, images, files, or links and they will appear here. Sensitive clipboard types are skipped."
            : "复制文字、图片、文件或链接后会显示在这里；密码等敏感剪贴板内容不会记录。"
    }
}

private struct ClipboardHistoryRow: View {
    let entry: ClipboardHistoryEntry
    let sourceName: String?
    let thumbnail: NSImage?
    let isSelected: Bool
    let isEnglish: Bool

    var body: some View {
        HStack(spacing: 12) {
            leadingPreview
            VStack(alignment: .leading, spacing: 3) {
                Text(title)
                    .font(.system(size: 14, weight: .semibold))
                    .lineLimit(entry.kind == .text ? 2 : 1)
                HStack(spacing: 7) {
                    if let sourceName { Text(sourceName) }
                    Text(entry.updatedAt, style: .relative)
                    Text(ByteCountFormatter.string(fromByteCount: entry.byteCount, countStyle: .file))
                }
                .font(.system(size: 11))
                .foregroundStyle(isSelected ? .white.opacity(0.72) : .secondary)
            }
            Spacer(minLength: 10)
            if entry.isPinned {
                Image(systemName: "pin.fill")
                    .font(.system(size: 11))
                    .foregroundStyle(isSelected ? .white.opacity(0.8) : .orange)
            }
        }
        .foregroundStyle(isSelected ? .white : .primary)
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .frame(minHeight: entry.kind == .image ? 60 : 52)
        .background(
            isSelected ? Color.accentColor.opacity(0.88) : Color.clear,
            in: RoundedRectangle(cornerRadius: 9, style: .continuous)
        )
    }

    @ViewBuilder
    private var leadingPreview: some View {
        if entry.kind == .image, let thumbnail {
            Image(nsImage: thumbnail)
                .resizable()
                .scaledToFill()
                .frame(width: 58, height: 44)
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                .overlay {
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .strokeBorder(isSelected ? .white.opacity(0.34) : .black.opacity(0.10), lineWidth: 1)
                }
        } else {
            Image(systemName: icon)
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(isSelected ? Color.white : Color.accentColor)
                .frame(width: 36, height: 36)
                .background(
                    isSelected ? .white.opacity(0.14) : Color.accentColor.opacity(0.10),
                    in: RoundedRectangle(cornerRadius: 8)
                )
        }
    }

    private var title: String {
        if let preview = entry.textPreview, !preview.isEmpty { return preview }
        return switch entry.kind {
        case .text: isEnglish ? "Text" : "文本"
        case .url: isEnglish ? "Link" : "链接"
        case .files: isEnglish ? "Files" : "文件"
        case .image: isEnglish ? "Image" : "图片"
        }
    }

    private var icon: String {
        switch entry.kind {
        case .text: "text.alignleft"
        case .url: "link"
        case .files: "doc.on.doc.fill"
        case .image: "photo.fill"
        }
    }
}
