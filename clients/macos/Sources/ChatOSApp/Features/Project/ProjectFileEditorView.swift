import AppKit
import ChatOSCore
import SwiftUI

struct ProjectFileEditorView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: ProjectDirectoryViewModel

    var body: some View {
        VStack(spacing: 0) {
            if let file = viewModel.selectedFile {
                header(file)
                    .background(AppPalette.surface)
                    .zIndex(1)
                Divider()
                if shouldShowCodeNavigation(for: file) {
                    codeNavigationPanel
                    Divider()
                }
                content(file)
                    .clipped()
            } else {
                ContentUnavailableView(
                    model.localized("选择一个文件", english: "Select a file"),
                    systemImage: "doc.text.magnifyingglass",
                    description: Text(model.localized(
                        "从左侧项目目录中选择文件以预览或编辑。",
                        english: "Select a file from the project directory to preview or edit it."
                    ))
                )
                    .workspaceFill()
            }
        }
        .workspaceFill()
    }

    private func shouldShowCodeNavigation(for file: ProjectFileContent) -> Bool {
        guard !file.isBinary else { return false }
        return viewModel.selectedSymbol != nil
            || viewModel.navigationResult != nil
            || viewModel.navigationError != nil
            || viewModel.canNavigateBack
    }

    private var codeNavigationPanel: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                if let symbol = viewModel.selectedSymbol {
                    Label(symbol.token, systemImage: "scope")
                        .appFont(.caption.monospaced().weight(.semibold))
                        .padding(.horizontal, 9)
                        .padding(.vertical, 5)
                        .background(Color.accentColor.opacity(0.1), in: Capsule())
                }

                Button(model.localized("转到定义", english: "Go to Definition"), systemImage: "arrow.turn.down.right") {
                    Task { await viewModel.requestNavigation(.definition) }
                }
                .disabled(viewModel.selectedSymbol == nil || viewModel.isNavigating)

                Button(model.localized("查找引用", english: "Find References"), systemImage: "point.3.connected.trianglepath.dotted") {
                    Task { await viewModel.requestNavigation(.references) }
                }
                .disabled(viewModel.selectedSymbol == nil || viewModel.isNavigating)

                Button(model.localized("返回", english: "Back"), systemImage: "chevron.backward") {
                    Task { await viewModel.navigateBack() }
                }
                .keyboardShortcut("[", modifiers: .command)
                .disabled(!viewModel.canNavigateBack || viewModel.isNavigating)
                .help(model.localized("返回上一个位置（⌘[）", english: "Return to the previous location (⌘[)"))

                if viewModel.isNavigating { ProgressView().controlSize(.small) }
                Spacer()

                if let result = viewModel.navigationResult,
                   let kind = viewModel.navigationRequestKind {
                    Text("\(kind.rawValue) · \(result.locations.count) 处 · \(localizedMode(result.mode))")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }

                Button {
                    viewModel.clearNavigationResults()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .help(model.localized("关闭代码导航", english: "Close code navigation"))
            }

            if let error = viewModel.navigationError {
                Text(error)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }

            if let locations = viewModel.navigationResult?.locations, !locations.isEmpty {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(locations) { location in
                            Button {
                                Task { await viewModel.openNavigationLocation(location) }
                            } label: {
                                HStack(alignment: .top, spacing: 10) {
                                    Image(systemName: "arrow.right.circle")
                                        .foregroundStyle(Color.accentColor)
                                        .padding(.top, 1)
                                    VStack(alignment: .leading, spacing: 3) {
                                        Text(model.localized(
                                            "\(location.relativePath) · 第 \(location.line) 行",
                                            english: "\(location.relativePath) · Line \(location.line)"
                                        ))
                                            .appFont(.caption.weight(.semibold))
                                        Text(location.preview.trimmingCharacters(in: .whitespaces))
                                            .appFont(.caption.monospaced())
                                            .foregroundStyle(.secondary)
                                            .lineLimit(2)
                                            .frame(maxWidth: .infinity, alignment: .leading)
                                    }
                                }
                                .padding(.horizontal, 10)
                                .padding(.vertical, 7)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            if location.id != locations.last?.id { Divider() }
                        }
                    }
                }
                .frame(maxHeight: 180)
                .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
                .overlay { RoundedRectangle(cornerRadius: 8).stroke(.separator) }
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .background(AppPalette.surfaceSubtle)
    }

    private func localizedMode(_ mode: String) -> String {
        switch mode {
        case "heuristic": model.localized("智能定位", english: "Smart location")
        case "text-search": model.localized("全文符号搜索", english: "Full-text symbol search")
        default: mode
        }
    }

    private func header(_ file: ProjectFileContent) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(file.name).appFont(.headline)
                Text(file.displayPath ?? file.name)
                    .appFont(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .help(file.displayPath ?? file.name)
            }
            Spacer()
            if viewModel.isEditing {
                Button(model.localized("取消", english: "Cancel")) { viewModel.cancelEditing() }
                Button(
                    viewModel.isSaving
                        ? model.localized("保存中…", english: "Saving…")
                        : model.localized("保存", english: "Save"),
                    systemImage: "square.and.arrow.down"
                ) {
                    Task { await viewModel.save() }
                }
                .buttonStyle(.borderedProminent)
                .disabled(viewModel.isSaving)
            } else {
                Button(model.localized("复制", english: "Copy"), systemImage: "doc.on.doc") {
                    copyToPasteboard(file)
                }
                Menu {
                    Button(model.localized("在 Finder 中显示", english: "Show in Finder"), systemImage: "folder") { Task { await viewModel.openSelected(mode: .reveal) } }
                    Button(model.localized("使用默认应用打开", english: "Open with Default App"), systemImage: "arrow.up.forward.app") { Task { await viewModel.openSelected(mode: .default) } }
                    Divider()
                    Button(model.localized("删除", english: "Delete"), systemImage: "trash", role: .destructive) { Task { await viewModel.deleteSelected() } }
                } label: { Image(systemName: "ellipsis.circle") }
                if file.isWritable && !file.isBinary {
                    Button(model.localized("编辑", english: "Edit"), systemImage: "pencil") { viewModel.beginEditing() }
                        .buttonStyle(.borderedProminent)
                }
            }
        }
        .padding(14)
    }

    @ViewBuilder
    private func content(_ file: ProjectFileContent) -> some View {
        if viewModel.isEditing {
            CodeEditorView(
                text: $viewModel.draft,
                fileName: file.name,
                targetLine: viewModel.selectedLine,
                onSymbolSelection: viewModel.selectSymbol
            )
            .id(file.path)
            .clipped()
        } else if file.supportsImagePreview {
            ProjectImagePreview(file: file)
        } else if file.isBinary {
            ContentUnavailableView(
                model.localized("二进制文件", english: "Binary File"),
                systemImage: "doc.zipper",
                description: Text(model.localized(
                    "此文件不能在文本编辑器中显示。",
                    english: "This file cannot be displayed in the text editor."
                ))
            )
                .workspaceFill()
        } else {
            if ["md", "markdown"].contains(URL(fileURLWithPath: file.name).pathExtension.lowercased()),
               viewModel.selectedLine == nil {
                ScrollView {
                    MarkdownDocumentView(markdown: file.content)
                        .padding(24)
                        .frame(maxWidth: .infinity, alignment: .topLeading)
                }
            } else {
                CodePreviewView(
                    content: file.content,
                    fileName: file.name,
                    targetLine: viewModel.selectedLine,
                    onSymbolSelection: viewModel.selectSymbol
                )
                .id(file.path)
                .clipped()
            }
        }
    }

    private func copyToPasteboard(_ file: ProjectFileContent) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        if file.supportsImagePreview,
           let data = file.imagePreviewData,
           let image = NSImage(data: data) {
            pasteboard.writeObjects([image])
        } else {
            pasteboard.setString(file.content, forType: .string)
        }
    }
}

private struct ProjectImagePreview: View {
    @EnvironmentObject private var model: AppModel
    let file: ProjectFileContent

    private var image: NSImage? {
        file.imagePreviewData.flatMap(NSImage.init(data:))
    }

    var body: some View {
        if let image {
            GeometryReader { geometry in
                ZStack {
                    CheckerboardBackground()
                    Image(nsImage: image)
                        .resizable()
                        .interpolation(.high)
                        .aspectRatio(contentMode: .fit)
                        .frame(
                            maxWidth: max(geometry.size.width - 48, 1),
                            maxHeight: max(geometry.size.height - 48, 1)
                        )
                        .padding(24)
                }
            }
            .workspaceFill()
        } else {
            ContentUnavailableView(
                model.localized("无法预览图片", english: "Unable to preview image"),
                systemImage: "photo.badge.exclamationmark",
                description: Text(model.localized(
                    "图片数据无效，或系统暂不支持此图片格式。",
                    english: "The image data is invalid or this format is not currently supported."
                ))
            )
            .workspaceFill()
        }
    }
}

private struct CheckerboardBackground: View {
    private let tileSize: CGFloat = 12

    var body: some View {
        Canvas { context, size in
            context.fill(Path(CGRect(origin: .zero, size: size)), with: .color(Color(nsColor: .textBackgroundColor)))
            let columns = Int(ceil(size.width / tileSize))
            let rows = Int(ceil(size.height / tileSize))
            for row in 0..<rows {
                for column in 0..<columns where (row + column).isMultiple(of: 2) {
                    let rect = CGRect(
                        x: CGFloat(column) * tileSize,
                        y: CGFloat(row) * tileSize,
                        width: tileSize,
                        height: tileSize
                    )
                    context.fill(Path(rect), with: .color(Color.secondary.opacity(0.055)))
                }
            }
        }
        .accessibilityHidden(true)
    }
}

private extension ProjectFileContent {
    static let previewableImageExtensions: Set<String> = [
        "avif", "bmp", "gif", "heic", "heif", "ico", "jpeg", "jpg",
        "png", "svg", "tif", "tiff", "webp",
    ]

    var supportsImagePreview: Bool {
        if contentType?.lowercased().hasPrefix("image/") == true { return true }
        if Self.previewableImageExtensions.contains(
            URL(fileURLWithPath: name).pathExtension.lowercased()
        ) {
            return true
        }
        guard isBinary, let data = imagePreviewData else { return false }
        return NSImage(data: data) != nil
    }

    var imagePreviewData: Data? {
        if isBinary {
            return Data(base64Encoded: content, options: .ignoreUnknownCharacters)
        }
        return Data(content.utf8)
    }
}
