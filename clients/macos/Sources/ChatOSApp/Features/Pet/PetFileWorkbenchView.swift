import AppKit
import ChatOSCore
import SwiftUI

struct PetFileWorkbenchView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var store: PetFileWorkbenchStore
    @ObservedObject var defaultHandlerPrompt: PetDefaultFileHandlerPromptController

    var body: some View {
        VStack(spacing: 0) {
            titleBar
            Divider()
            tabBar
            if let prompt = defaultHandlerPrompt.prompt {
                Divider()
                defaultHandlerBanner(prompt)
            }
            Divider()
            if let tab = store.selectedTab {
                fileToolbar(tab)
                Divider()
                if let error = tab.errorMessage, case .ready = tab.loadState {
                    errorBanner(error)
                    Divider()
                }
                tabContent(tab)
            } else {
                ContentUnavailableView(
                    model.localized("没有打开的文件", english: "No Open Files"),
                    systemImage: "doc.on.doc",
                    description: Text(model.localized(
                        "可从项目目录或 Finder 中选择“在宠物中打开”。",
                        english: "Choose Open in Pet from the project directory or Finder."
                    ))
                )
                .workspaceFill()
            }
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 16))
        .overlay {
            RoundedRectangle(cornerRadius: 16)
                .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
        }
        .shadow(color: .black.opacity(0.3), radius: 22, y: 12)
        .alert(
            closeAlertTitle,
            isPresented: closeAlertPresented,
            presenting: store.pendingCloseRequest
        ) { _ in
            Button(model.localized("保存", english: "Save")) {
                Task { await store.savePendingClose() }
            }
            Button(
                model.localized("不保存", english: "Don't Save"),
                role: .destructive
            ) {
                store.discardPendingClose()
            }
            Button(model.localized("取消", english: "Cancel"), role: .cancel) {
                store.cancelPendingClose()
            }
        } message: { request in
            Text(closeAlertMessage(request))
        }
        .alert(
            model.localized("文件已在其他位置修改", english: "File Changed Elsewhere"),
            isPresented: conflictAlertPresented,
            presenting: store.saveConflict
        ) { _ in
            Button(model.localized("重新载入", english: "Reload")) {
                store.reloadConflictingFile()
            }
            Button(model.localized("覆盖保存", english: "Overwrite"), role: .destructive) {
                Task { await store.overwriteConflictingFile() }
            }
            Button(model.localized("取消", english: "Cancel"), role: .cancel) {
                store.cancelSaveConflict()
            }
        } message: { _ in
            Text(model.localized(
                "磁盘上的内容与打开文件时不同。可以重新载入磁盘版本，或用当前编辑内容覆盖它。",
                english: "The file on disk differs from the version you opened. Reload it or overwrite it with your current edits."
            ))
        }
    }

    private func defaultHandlerBanner(
        _ prompt: PetDefaultFileHandlerPromptController.Prompt
    ) -> some View {
        HStack(spacing: 10) {
            Text(defaultHandlerQuestion(prompt))
                .appFont(.caption.weight(.medium))
            Spacer()
            Button(model.localized("仅本次", english: "Just Once")) {
                defaultHandlerPrompt.dismiss()
            }
            .controlSize(.small)
            Button(model.localized("设为默认", english: "Make Default")) {
                defaultHandlerPrompt.makeDefault()
            }
            .controlSize(.small)
            .buttonStyle(.borderedProminent)
        }
        .padding(.horizontal, 14)
        .frame(height: 42)
        .background(AppPalette.ai.opacity(0.08))
    }

    private func defaultHandlerQuestion(
        _ prompt: PetDefaultFileHandlerPromptController.Prompt
    ) -> String {
        if prompt.fileExtension.isEmpty {
            return model.localized(
                "以后默认用 ChatOS 打开这类文件？",
                english: "Always open this file type with ChatOS?"
            )
        }
        return model.localized(
            "以后默认用 ChatOS 打开 .\(prompt.fileExtension)？",
            english: "Always open .\(prompt.fileExtension) with ChatOS?"
        )
    }

    private var titleBar: some View {
        HStack(spacing: 10) {
            Image(systemName: "pawprint.fill")
                .foregroundStyle(AppPalette.ai)
            Text(model.localized("宠物文件台", english: "Pet File Desk"))
                .appFont(.headline)
            if store.hasDirtyTabs {
                Text(model.localized("有未保存修改", english: "Unsaved changes"))
                    .appFont(.caption2.weight(.semibold))
                    .foregroundStyle(.orange)
            }
            Spacer()
            Button {
                store.requestDismiss()
            } label: {
                Image(systemName: "xmark")
                    .frame(width: 22, height: 22)
            }
            .buttonStyle(.plain)
            .keyboardShortcut(.escape, modifiers: [])
            .help(model.localized("关闭文件台", english: "Close file desk"))
        }
        .padding(.horizontal, 14)
        .frame(height: 44)
        .background(.bar)
    }

    private var tabBar: some View {
        ScrollView(.horizontal) {
            HStack(spacing: 3) {
                ForEach(store.tabs) { tab in
                    PetFileTabButton(
                        tab: tab,
                        isSelected: store.selectedTabID == tab.id,
                        onSelect: { store.selectTab(tab.id) },
                        onClose: { store.requestCloseTab(tab.id) }
                    )
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
        }
        .scrollIndicators(.hidden)
        .background(AppPalette.surfaceSubtle)
    }

    private func fileToolbar(_ tab: PetFileTab) -> some View {
        HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(tab.name)
                    .appFont(.subheadline.weight(.semibold))
                    .lineLimit(1)
                Text(tab.file?.displayPath ?? tab.path)
                    .appFont(.caption2.monospaced())
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .help(tab.file?.displayPath ?? tab.path)
            }
            Spacer()

            if tab.isEditing {
                Button(model.localized("取消编辑", english: "Cancel Editing")) {
                    store.cancelEditing(tabID: tab.id)
                }
                Button(
                    tab.isSaving
                        ? model.localized("保存中…", english: "Saving…")
                        : model.localized("保存", english: "Save"),
                    systemImage: "square.and.arrow.down"
                ) {
                    Task { await store.save(tabID: tab.id) }
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut("s", modifiers: .command)
                .disabled(tab.isSaving || !tab.isDirty)
            } else if tab.file?.isWritable == true, tab.file?.isBinary == false {
                Button(model.localized("编辑", english: "Edit"), systemImage: "pencil") {
                    store.beginEditing(tabID: tab.id)
                }
                .buttonStyle(.borderedProminent)
            }

            Button {
                store.reload(tabID: tab.id)
            } label: {
                Image(systemName: "arrow.clockwise")
            }
            .disabled(tab.isSaving || tab.isDirty)
            .help(model.localized("从磁盘重新载入", english: "Reload from disk"))

            Menu {
                Button(
                    model.localized("在 Finder 中显示", english: "Show in Finder"),
                    systemImage: "folder"
                ) {
                    Task { await store.openExternally(.reveal) }
                }
                Button(
                    model.localized("使用默认应用打开", english: "Open with Default App"),
                    systemImage: "arrow.up.forward.app"
                ) {
                    Task { await store.openExternally(.default) }
                }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
        }
        .padding(.horizontal, 14)
        .frame(height: 56)
        .background(AppPalette.surface)
    }

    @ViewBuilder
    private func tabContent(_ tab: PetFileTab) -> some View {
        switch tab.loadState {
        case .loading:
            ProgressView(model.localized("正在打开文件…", english: "Opening file…"))
                .workspaceFill()
        case let .failed(message):
            ContentUnavailableView {
                Label(
                    model.localized("无法打开文件", english: "Unable to Open File"),
                    systemImage: "doc.badge.exclamationmark"
                )
            } description: {
                Text(message)
            } actions: {
                Button(model.localized("重试", english: "Retry")) {
                    store.retry(tabID: tab.id)
                }
            }
            .workspaceFill()
        case .ready:
            if let file = tab.file {
                readyContent(tab: tab, file: file)
            }
        }
    }

    @ViewBuilder
    private func readyContent(tab: PetFileTab, file: ProjectFileContent) -> some View {
        if tab.isEditing {
            CodeEditorView(
                text: draftBinding(tab.id),
                fileName: file.name,
                targetLine: tab.targetLine
            )
            .id(tab.id)
            .clipped()
        } else if let image = PetFilePreviewResolver.image(for: file) {
            PetFileImagePreview(image: image, fileName: file.name)
                .id(tab.id)
        } else if file.isBinary {
            ContentUnavailableView(
                model.localized("暂不支持预览", english: "Preview Not Supported"),
                systemImage: "doc.zipper",
                description: Text(model.localized(
                    "这是二进制文件，可以使用默认应用打开。",
                    english: "This is a binary file. You can open it with its default application."
                ))
            )
            .workspaceFill()
        } else if ["md", "markdown"].contains(
            URL(fileURLWithPath: file.name).pathExtension.lowercased()
        ), tab.targetLine == nil {
            ScrollView {
                MarkdownDocumentView(markdown: file.content)
                    .padding(24)
                    .frame(maxWidth: .infinity, alignment: .topLeading)
            }
        } else {
            CodePreviewView(
                content: file.content,
                fileName: file.name,
                targetLine: tab.targetLine
            )
            .id(tab.id)
            .clipped()
        }
    }

    private func errorBanner(_ message: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.orange)
            Text(message)
                .appFont(.caption)
                .lineLimit(2)
            Spacer()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(Color.orange.opacity(0.08))
    }

    private func draftBinding(_ tabID: String) -> Binding<String> {
        Binding(
            get: { store.tabs.first(where: { $0.id == tabID })?.draft ?? "" },
            set: { store.updateDraft($0, tabID: tabID) }
        )
    }

    private var closeAlertPresented: Binding<Bool> {
        Binding(
            get: { store.pendingCloseRequest != nil },
            set: { if !$0 { store.cancelPendingClose() } }
        )
    }

    private var conflictAlertPresented: Binding<Bool> {
        Binding(
            get: { store.saveConflict != nil },
            set: { if !$0 { store.cancelSaveConflict() } }
        )
    }

    private var closeAlertTitle: String {
        guard let request = store.pendingCloseRequest else {
            return model.localized("保存修改？", english: "Save Changes?")
        }
        switch request.target {
        case .tab:
            return model.localized("关闭前保存修改？", english: "Save Before Closing?")
        case .workbench:
            return model.localized("关闭文件台前保存修改？", english: "Save Before Closing the File Desk?")
        }
    }

    private func closeAlertMessage(_ request: PetFileCloseRequest) -> String {
        switch request.target {
        case let .tab(id):
            let name = store.tabs.first(where: { $0.id == id })?.name ?? id
            return model.localized(
                "“\(name)”包含尚未保存的修改。",
                english: "“\(name)” contains unsaved changes."
            )
        case .workbench:
            return model.localized(
                "一个或多个标签包含尚未保存的修改。",
                english: "One or more tabs contain unsaved changes."
            )
        }
    }
}

private struct PetFileTabButton: View {
    let tab: PetFileTab
    let isSelected: Bool
    let onSelect: () -> Void
    let onClose: () -> Void

    var body: some View {
        HStack(spacing: 7) {
            Button(action: onSelect) {
                HStack(spacing: 7) {
                    Image(systemName: icon)
                        .foregroundStyle(isSelected ? Color.accentColor : .secondary)
                    Text(tab.name)
                        .lineLimit(1)
                    if tab.isDirty {
                        Circle()
                            .fill(Color.orange)
                            .frame(width: 6, height: 6)
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            Button(action: onClose) {
                Image(systemName: "xmark")
                    .font(.system(size: 9, weight: .semibold))
                    .frame(width: 16, height: 16)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
        }
        .appFont(.caption)
        .padding(.leading, 10)
        .padding(.trailing, 6)
        .frame(height: 30)
        .background(
            isSelected ? Color(nsColor: .windowBackgroundColor) : Color.clear,
            in: RoundedRectangle(cornerRadius: 7)
        )
        .overlay {
            if isSelected {
                RoundedRectangle(cornerRadius: 7)
                    .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
            }
        }
    }

    private var icon: String {
        if PetFilePreviewResolver.isImageFileName(tab.name) { return "photo" }
        switch URL(fileURLWithPath: tab.name).pathExtension.lowercased() {
        case "swift": return "swift"
        case "md", "markdown": return "doc.richtext"
        case "json", "yaml", "yml", "toml": return "curlybraces"
        default: return "doc.text"
        }
    }
}

private struct PetFileImagePreview: View {
    @EnvironmentObject private var model: AppModel
    let image: NSImage
    let fileName: String
    @State private var zoom: CGFloat = 1

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Button { zoom = max(0.25, zoom - 0.25) } label: {
                    Image(systemName: "minus.magnifyingglass")
                }
                Button {
                    zoom = 1
                } label: {
                    Text("\(Int(zoom * 100))%")
                        .appFont(.caption.monospacedDigit())
                        .frame(minWidth: 42)
                }
                Button { zoom = min(4, zoom + 0.25) } label: {
                    Image(systemName: "plus.magnifyingglass")
                }
                Spacer()
                Text(model.localized(
                    "\(Int(image.size.width)) × \(Int(image.size.height))",
                    english: "\(Int(image.size.width)) × \(Int(image.size.height))"
                ))
                    .appFont(.caption2.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .padding(.horizontal, 14)
            .frame(height: 34)
            .background(AppPalette.surfaceSubtle)
            Divider()

            GeometryReader { geometry in
                ScrollView([.horizontal, .vertical]) {
                    Image(nsImage: image)
                        .resizable()
                        .interpolation(.high)
                        .aspectRatio(contentMode: .fit)
                        .frame(
                            width: max(1, geometry.size.width - 48),
                            height: max(1, geometry.size.height - 48)
                        )
                        .scaleEffect(zoom)
                        .frame(
                            width: max(geometry.size.width, (geometry.size.width - 48) * zoom),
                            height: max(geometry.size.height, (geometry.size.height - 48) * zoom)
                        )
                        .padding(24)
                }
                .background(PetCheckerboardBackground())
            }
        }
    }
}

private struct PetCheckerboardBackground: View {
    private let tileSize: CGFloat = 12

    var body: some View {
        Canvas { context, size in
            context.fill(
                Path(CGRect(origin: .zero, size: size)),
                with: .color(Color(nsColor: .textBackgroundColor))
            )
            let columns = Int(ceil(size.width / tileSize))
            let rows = Int(ceil(size.height / tileSize))
            for row in 0..<rows {
                for column in 0..<columns where (row + column).isMultiple(of: 2) {
                    context.fill(
                        Path(CGRect(
                            x: CGFloat(column) * tileSize,
                            y: CGFloat(row) * tileSize,
                            width: tileSize,
                            height: tileSize
                        )),
                        with: .color(Color.secondary.opacity(0.055))
                    )
                }
            }
        }
        .accessibilityHidden(true)
    }
}

private enum PetFilePreviewResolver {
    private static let imageExtensions: Set<String> = [
        "avif", "bmp", "gif", "heic", "heif", "ico", "jpeg", "jpg",
        "png", "svg", "tif", "tiff", "webp",
    ]

    static func isImageFileName(_ name: String) -> Bool {
        imageExtensions.contains(URL(fileURLWithPath: name).pathExtension.lowercased())
    }

    static func image(for file: ProjectFileContent) -> NSImage? {
        let looksLikeImage = file.contentType?.lowercased().hasPrefix("image/") == true
            || isImageFileName(file.name)
        guard looksLikeImage || file.isBinary else { return nil }
        let data = file.isBinary
            ? Data(base64Encoded: file.content, options: .ignoreUnknownCharacters)
            : Data(file.content.utf8)
        return data.flatMap(NSImage.init(data:))
    }
}
