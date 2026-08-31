import AppKit
import ChatOSCore
import SwiftUI

struct QuickSearchView: View {
    @ObservedObject var viewModel: QuickSearchViewModel
    let isEnglish: Bool
    let shortcutLabel: String

    var body: some View {
        VStack(spacing: 0) {
            searchHeader
            Divider().opacity(0.65)
            resultContent
            Divider().opacity(0.65)
            footer
        }
        .frame(width: 760, height: 520)
        .background(.ultraThickMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .strokeBorder(.white.opacity(0.16), lineWidth: 1)
        }
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private var searchHeader: some View {
        HStack(spacing: 14) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 22, weight: .medium))
                .foregroundStyle(.secondary)
            GlobalCommandSearchField(
                text: Binding(
                    get: { viewModel.query },
                    set: { viewModel.updateQuery($0) }
                ),
                placeholder: isEnglish
                    ? "Search apps, files, ChatOS, and actions"
                    : "搜索应用、文件、ChatOS 和操作",
                fontSize: 21,
                onMove: viewModel.moveSelection,
                onSubmit: viewModel.executeSelected,
                onCancel: viewModel.cancel
            )
            if viewModel.isSearchingFiles {
                ProgressView().controlSize(.small)
            }
            Text(shortcutLabel)
                .font(.system(size: 11, weight: .semibold, design: .rounded))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 8)
                .padding(.vertical, 5)
                .background(.quaternary, in: RoundedRectangle(cornerRadius: 6))
        }
        .padding(.horizontal, 20)
        .frame(height: 68)
    }

    @ViewBuilder
    private var resultContent: some View {
        if viewModel.results.isEmpty {
            VStack(spacing: 10) {
                Image(systemName: "sparkle.magnifyingglass")
                    .font(.system(size: 30))
                    .foregroundStyle(.secondary)
                Text(viewModel.diagnostic ?? (isEnglish ? "Type to search your Mac" : "输入内容搜索这台 Mac"))
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 4) {
                        ForEach(Array(viewModel.results.enumerated()), id: \.element.id) { index, result in
                            if index == 0 || viewModel.results[index - 1].kind != result.kind {
                                Text(groupTitle(result.kind))
                                    .font(.system(size: 11, weight: .semibold))
                                    .foregroundStyle(.tertiary)
                                    .textCase(.uppercase)
                                    .padding(.horizontal, 14)
                                    .padding(.top, index == 0 ? 8 : 12)
                                    .padding(.bottom, 2)
                            }
                            QuickSearchResultRow(
                                result: result,
                                isSelected: viewModel.selectedIndex == index
                            )
                            .id(result.id)
                            .contentShape(Rectangle())
                            .onTapGesture { viewModel.select(index) }
                            .onTapGesture(count: 2) { viewModel.execute(result) }
                        }
                    }
                    .padding(.horizontal, 8)
                    .padding(.bottom, 10)
                }
                .onChange(of: viewModel.selectedIndex) { _, index in
                    guard viewModel.results.indices.contains(index) else { return }
                    withAnimation(.easeOut(duration: 0.08)) {
                        proxy.scrollTo(viewModel.results[index].id, anchor: .center)
                    }
                }
            }
        }
    }

    private var footer: some View {
        HStack(spacing: 18) {
            Label(isEnglish ? "Actions" : "操作", systemImage: "command")
            Spacer()
            keyHint("↑↓", isEnglish ? "Navigate" : "选择")
            keyHint("↩", isEnglish ? "Open" : "打开")
            keyHint("esc", isEnglish ? "Close" : "关闭")
        }
        .font(.system(size: 11, weight: .medium))
        .foregroundStyle(.secondary)
        .padding(.horizontal, 16)
        .frame(height: 38)
    }

    private func keyHint(_ key: String, _ label: String) -> some View {
        HStack(spacing: 5) {
            Text(key)
                .font(.system(size: 10, weight: .bold, design: .rounded))
                .padding(.horizontal, 5)
                .padding(.vertical, 2)
                .background(.quaternary, in: RoundedRectangle(cornerRadius: 4))
            Text(label)
        }
    }

    private func groupTitle(_ kind: QuickSearchResultKind) -> String {
        switch kind {
        case .suggestion: isEnglish ? "Suggestions" : "建议"
        case .chatOS: "ChatOS"
        case .application: isEnglish ? "Applications" : "应用程序"
        case .file: isEnglish ? "Files & Folders" : "文件与文件夹"
        case .action: isEnglish ? "Actions" : "操作"
        }
    }
}

private struct QuickSearchResultRow: View {
    let result: QuickSearchResult
    let isSelected: Bool

    var body: some View {
        HStack(spacing: 12) {
            icon
                .frame(width: 34, height: 34)
            VStack(alignment: .leading, spacing: 2) {
                Text(result.title)
                    .font(.system(size: 14, weight: .semibold))
                    .lineLimit(1)
                if let subtitle = result.subtitle, !subtitle.isEmpty {
                    Text(subtitle)
                        .font(.system(size: 11.5))
                        .foregroundStyle(isSelected ? .white.opacity(0.72) : .secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            Spacer(minLength: 12)
            if isSelected {
                Image(systemName: "return")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(.white.opacity(0.8))
            }
        }
        .foregroundStyle(isSelected ? .white : .primary)
        .padding(.horizontal, 12)
        .frame(height: 48)
        .background(
            isSelected ? Color.accentColor.opacity(0.88) : Color.clear,
            in: RoundedRectangle(cornerRadius: 9, style: .continuous)
        )
    }

    @ViewBuilder
    private var icon: some View {
        switch result.action {
        case let .openApplication(url), let .openFile(url):
            Image(nsImage: NSWorkspace.shared.icon(forFile: url.path))
                .resizable()
                .scaledToFit()
        default:
            Image(systemName: result.systemImage)
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(isSelected ? Color.white : Color.accentColor)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(
                    isSelected ? .white.opacity(0.15) : Color.accentColor.opacity(0.10),
                    in: RoundedRectangle(cornerRadius: 8)
                )
        }
    }
}
