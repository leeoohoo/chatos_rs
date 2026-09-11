import AppKit
import SwiftUI

/// Product-facing audit surface for every prompt used by Story Studio.
/// The catalog invokes the same builders as generation and planning requests.
struct StoryPromptInspectorView: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var selectedCategory: StoryPromptCatalog.Category?
    @State private var selectedItemID: String?
    @State private var query = ""

    private var allItems: [StoryPromptCatalog.Item] {
        StoryPromptCatalog.items()
    }

    private var visibleItems: [StoryPromptCatalog.Item] {
        allItems.filter { item in
            let categoryMatches = selectedCategory == nil || item.category == selectedCategory
            let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
            guard categoryMatches, !needle.isEmpty else { return categoryMatches }
            return [item.registryKey, item.title, item.usedWhen, item.trigger, item.implementation, item.content ?? ""]
                .contains { $0.localizedCaseInsensitiveContains(needle) }
        }
    }

    private var selectedItem: StoryPromptCatalog.Item? {
        if let selectedItemID, let item = visibleItems.first(where: { $0.id == selectedItemID }) { return item }
        return visibleItems.first
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            configurationBar
            Divider()
            HStack(spacing: 0) {
                categorySidebar
                Divider()
                promptList
            }
        }
        .frame(minWidth: 1_050, idealWidth: 1_280, minHeight: 720, idealHeight: 820)
        .background(Color(nsColor: .windowBackgroundColor))
        .onChange(of: visibleItems.map(\.id), initial: true) { _, ids in
            if let selectedItemID {
                if !ids.contains(selectedItemID) { self.selectedItemID = ids.first }
            } else {
                selectedItemID = ids.first
            }
        }
    }

    private var header: some View {
        HStack(spacing: 14) {
            ZStack {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(LinearGradient(colors: [.indigo, .purple], startPoint: .topLeading, endPoint: .bottomTrailing))
                Image(systemName: "text.quote").font(.system(size: 20, weight: .semibold)).foregroundStyle(.white)
            }.frame(width: 46, height: 46)
            VStack(alignment: .leading, spacing: 4) {
                Text(appModel.localized("剧情提示词中心", english: "Story Prompt Center")).font(.title2.bold())
                Text(appModel.localized(
                    "统一审查生产环境使用的全局 Prompt 模板和 Agent 工具协议；这里不会调用模型或产生费用。",
                    english: "Audit the global prompt templates and Agent tool contracts used in production. Nothing here calls a model or incurs charges."
                )).font(.callout).foregroundStyle(.secondary)
            }
            Spacer()
            Text(appModel.localized("只读", english: "Read Only"))
                .font(.caption.weight(.semibold)).foregroundStyle(.green)
                .padding(.horizontal, 9).padding(.vertical, 5)
                .background(Color.green.opacity(0.1), in: Capsule())
            Button(appModel.localized("关闭", english: "Close")) { dismiss() }
                .keyboardShortcut(.cancelAction)
        }
        .padding(.horizontal, 22).padding(.vertical, 18)
        .background(.regularMaterial)
    }

    private var configurationBar: some View {
        HStack(spacing: 14) {
            Label(appModel.localized("全局注册表", english: "Global Registry"), systemImage: "tablecells")
                .font(.callout.weight(.semibold))
            Text(appModel.localized(
                "具体剧情、人物、场景和分段只在运行时注入模板，不属于 Prompt 配置。",
                english: "Stories, characters, scenes and segments are injected only at runtime; they are not prompt configuration."
            )).font(.callout).foregroundStyle(.secondary)
            Spacer()
            HStack(spacing: 7) {
                Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                TextField(appModel.localized("搜索名称、入口或代码位置", english: "Search title, trigger or code"), text: $query)
                    .textFieldStyle(.plain)
                if !query.isEmpty {
                    Button { query = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary) }
                        .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 10).padding(.vertical, 8).frame(width: 285)
            .background(Color.primary.opacity(0.045), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
        }
        .padding(.horizontal, 22).padding(.vertical, 13)
    }

    private var categorySidebar: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(appModel.localized("使用阶段", english: "STAGES"))
                .font(.caption2.weight(.bold)).foregroundStyle(.secondary).padding(.horizontal, 10).padding(.bottom, 3)
            categoryButton(nil, title: appModel.localized("全部提示词", english: "All Prompts"), icon: "square.grid.2x2", color: .indigo)
            ForEach(StoryPromptCatalog.Category.allCases) { category in
                categoryButton(category, title: categoryTitle(category), icon: categoryIcon(category), color: categoryColor(category))
            }
            Spacer()
            VStack(alignment: .leading, spacing: 7) {
                Label(appModel.localized("真实来源", english: "Live Sources"), systemImage: "checkmark.seal.fill")
                    .font(.caption.weight(.semibold)).foregroundStyle(.green)
                Text(appModel.localized(
                    "业务代码按 Key 读取这些模板；工具项直接读取当前 JSON Schema。",
                    english: "Production code reads these templates by key; tool entries read the current JSON Schema."
                )).font(.caption2).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            .padding(11).background(Color.green.opacity(0.06), in: RoundedRectangle(cornerRadius: 10))
        }
        .padding(16).frame(width: 210)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.45))
    }

    private func categoryButton(_ category: StoryPromptCatalog.Category?, title: String,
                                icon: String, color: Color) -> some View {
        let selected = selectedCategory == category
        let count = category.map { value in allItems.filter { $0.category == value }.count } ?? allItems.count
        return Button { selectedCategory = category } label: {
            HStack(spacing: 9) {
                Image(systemName: icon).frame(width: 18).foregroundStyle(selected ? color : .secondary)
                Text(title).font(.callout.weight(selected ? .semibold : .regular))
                Spacer()
                Text("\(count)").font(.caption2.bold().monospacedDigit()).foregroundStyle(.secondary)
            }
            .padding(.horizontal, 10).padding(.vertical, 9)
            .background(selected ? color.opacity(0.1) : Color.clear,
                        in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        }.buttonStyle(.plain)
    }

    private var promptList: some View {
        Group {
            if visibleItems.isEmpty {
                ContentUnavailableView.search(text: query)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                VStack(spacing: 0) {
                    HStack(alignment: .firstTextBaseline) {
                        VStack(alignment: .leading, spacing: 3) {
                            Text(selectedCategory.map(categoryTitle) ?? appModel.localized("全部提示词", english: "All Prompts"))
                                .font(.title3.bold())
                            Text(appModel.localized(
                                "共 \(visibleItems.count) 项。上表用于定位，下面展示所选 Key 的生产模板。",
                                english: "\(visibleItems.count) entries. Use the table to locate one; its production template appears below."
                            )).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                    }.padding(.horizontal, 18).padding(.vertical, 13)

                    Table(visibleItems, selection: $selectedItemID) {
                        TableColumn("Key") { item in
                            Text(item.registryKey).font(.caption.monospaced()).lineLimit(1)
                        }.width(min: 190, ideal: 230)
                        TableColumn(appModel.localized("名称", english: "Name")) { item in
                            Text(item.title).lineLimit(1)
                        }.width(min: 150, ideal: 210)
                        TableColumn(appModel.localized("阶段", english: "Stage")) { item in
                            Text(categoryTitle(item.category)).foregroundStyle(categoryColor(item.category))
                        }.width(min: 80, ideal: 105)
                        TableColumn(appModel.localized("触发入口", english: "Trigger")) { item in
                            Text(item.trigger).lineLimit(1)
                        }.width(min: 150, ideal: 230)
                        TableColumn(appModel.localized("代码位置", english: "Implementation")) { item in
                            Text(item.implementation).font(.caption.monospaced()).lineLimit(1)
                        }.width(min: 170, ideal: 250)
                    }
                    .tableStyle(.inset(alternatesRowBackgrounds: true))
                    .frame(minHeight: 210, idealHeight: 280, maxHeight: 330)

                    Divider()
                    if let selectedItem {
                        ScrollView {
                            StoryPromptDetailView(item: selectedItem, color: categoryColor(selectedItem.category))
                                .padding(18).frame(maxWidth: 980)
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private func categoryTitle(_ category: StoryPromptCatalog.Category) -> String {
        switch category {
        case .planning: appModel.localized("文本规划", english: "Text Planning")
        case .tools: appModel.localized("Agent 工具", english: "Agent Tools")
        case .images: appModel.localized("图片生成", english: "Image Generation")
        case .videos: appModel.localized("视频生成", english: "Video Generation")
        }
    }

    private func categoryIcon(_ category: StoryPromptCatalog.Category) -> String {
        switch category {
        case .planning: "text.bubble"
        case .tools: "wrench.and.screwdriver"
        case .images: "photo.on.rectangle.angled"
        case .videos: "video"
        }
    }

    private func categoryColor(_ category: StoryPromptCatalog.Category) -> Color {
        switch category {
        case .planning: .indigo
        case .tools: .teal
        case .images: .orange
        case .videos: .blue
        }
    }
}

private struct StoryPromptDetailView: View {
    @EnvironmentObject private var appModel: AppModel
    let item: StoryPromptCatalog.Item
    let color: Color
    @State private var copied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 12) {
                RoundedRectangle(cornerRadius: 3).fill(color).frame(width: 4, height: 38)
                VStack(alignment: .leading, spacing: 4) {
                    Text(item.title).font(.headline).foregroundStyle(.primary)
                    Text(item.usedWhen).font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
            }
            Divider()
            VStack(alignment: .leading, spacing: 10) {
                metadataRow("Key", value: item.registryKey, icon: "key")
                metadataRow(appModel.localized("触发入口", english: "Trigger"), value: item.trigger, icon: "cursorarrow.click.2")
                metadataRow(appModel.localized("代码位置", english: "Implementation"), value: item.implementation, icon: "chevron.left.forwardslash.chevron.right")
            }
            if let content = item.content {
                HStack {
                        Label(appModel.localized("注册模板 / 工具协议", english: "Registered Template / Tool Contract"), systemImage: "doc.plaintext")
                        .font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                    Spacer()
                    Button {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(content, forType: .string)
                        copied = true
                        DispatchQueue.main.asyncAfter(deadline: .now() + 1.4) { copied = false }
                    } label: {
                        Label(copied ? appModel.localized("已复制", english: "Copied") : appModel.localized("复制", english: "Copy"),
                              systemImage: copied ? "checkmark" : "doc.on.doc")
                    }.buttonStyle(.borderless).font(.caption)
                }
                ScrollView([.vertical, .horizontal]) {
                    Text(content)
                        .font(.system(size: 12, design: .monospaced))
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .topLeading)
                        .padding(13)
                    }
                .frame(minHeight: 110, maxHeight: 360)
                .background(Color(nsColor: .textBackgroundColor).opacity(0.72),
                            in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 9, style: .continuous).stroke(Color.primary.opacity(0.08)))
            } else {
                Label(item.unavailableReason ?? appModel.localized("当前上下文不足，无法构建。", english: "The current context is insufficient."),
                      systemImage: "exclamationmark.triangle")
                    .font(.callout).foregroundStyle(.orange).textSelection(.enabled)
                    .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.orange.opacity(0.07), in: RoundedRectangle(cornerRadius: 9))
            }
        }
        .padding(16)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 13, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 13, style: .continuous).stroke(color.opacity(0.2)))
    }

    private func metadataRow(_ label: String, value: String, icon: String) -> some View {
        HStack(alignment: .top, spacing: 9) {
            Image(systemName: icon).frame(width: 16).foregroundStyle(color)
            Text(label + "：").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
            Text(value).font(.caption.monospaced()).textSelection(.enabled)
        }
    }
}
