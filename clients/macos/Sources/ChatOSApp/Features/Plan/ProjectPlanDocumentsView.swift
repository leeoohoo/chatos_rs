import AppKit
import ChatOSCore
import SwiftUI

struct ProjectPlanDocumentsView: View {
    @ObservedObject var viewModel: ProjectPlanViewModel

    var body: some View {
        Group {
            if viewModel.documents.isEmpty {
                ContentUnavailableView(
                    "这个需求还没有技术文档",
                    systemImage: "doc.text"
                )
            } else {
                HSplitView {
                    documentList
                        .frame(minWidth: 230, idealWidth: 270, maxWidth: 330)
                    documentContent
                        .frame(minWidth: 420)
                }
            }
        }
        .workspaceFill()
    }

    private var documentList: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("技术文档").appFont(.headline)
                    Text("\(viewModel.documents.count) 份文档")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }
            .padding(14)
            Divider()
            ScrollView {
                LazyVStack(spacing: 7) {
                    ForEach(viewModel.documents) { document in
                        documentRow(document)
                    }
                }
                .padding(8)
            }
            .background(AppPalette.canvas)
        }
        .workspaceFill()
        .background(AppPalette.surface)
    }

    private func documentRow(_ document: ProjectRequirementDocument) -> some View {
        let selected = document.id == viewModel.selectedDocumentID
        return Button {
            viewModel.selectedDocumentID = document.id
        } label: {
            VStack(alignment: .leading, spacing: 6) {
                Text(document.title)
                    .appFont(.subheadline.weight(.semibold))
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                HStack(spacing: 6) {
                    Text(documentTypeTitle(document.type))
                    Text("v\(document.version)")
                    if let updatedAt = document.updatedAt {
                        Text(updatedAt, style: .date)
                    }
                }
                .appFont(.caption2)
                .foregroundStyle(.secondary)
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                selected ? Color.accentColor.opacity(0.12) : AppPalette.surface,
                in: RoundedRectangle(cornerRadius: 9)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 9)
                    .stroke(selected ? Color.accentColor.opacity(0.5) : AppPalette.border.opacity(0.65))
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private var documentContent: some View {
        if let document = viewModel.selectedDocument {
            VStack(spacing: 0) {
                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 5) {
                        Text(document.title).appFont(.headline)
                        HStack(spacing: 7) {
                            StatusCapsule(title: documentTypeTitle(document.type), color: AppPalette.ai)
                            StatusCapsule(title: document.format, color: .secondary)
                            StatusCapsule(title: "v\(document.version)", color: .secondary)
                        }
                    }
                    Spacer()
                }
                .padding(16)
                Divider()
                documentPreview(document)
            }
        } else {
            ContentUnavailableView("选择一份文档", systemImage: "doc.text")
        }
    }

    @ViewBuilder
    private func documentPreview(_ document: ProjectRequirementDocument) -> some View {
        if DocumentBody.isSVG(document) {
            ScrollView([.horizontal, .vertical]) {
                DocumentBody(document: document)
                    .padding(24)
            }
            .workspaceFill(alignment: .topLeading)
            .background(AppPalette.surface)
        } else {
            ScrollView(.vertical) {
                HStack(alignment: .top, spacing: 0) {
                    Spacer(minLength: 0)
                    DocumentBody(document: document)
                        .frame(maxWidth: 1_120, alignment: .leading)
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 28)
                .padding(.top, 24)
                .padding(.bottom, 36)
                .frame(maxWidth: .infinity, alignment: .top)
            }
            .workspaceFill(alignment: .topLeading)
            .background(AppPalette.surface)
        }
    }

    private func documentTypeTitle(_ type: String) -> String {
        switch type.lowercased() {
        case "technical_overview": "技术概览"
        case "implementation_plan": "实施计划"
        case "ui_svg_preview": "界面设计"
        case "architecture_diagram": "架构图"
        case "flowchart": "流程图"
        case "sequence_diagram": "时序图"
        case "api_design": "接口设计"
        case "data_model": "数据模型"
        case "risk_notes": "风险说明"
        default: "技术文档"
        }
    }
}

private struct DocumentBody: View {
    let document: ProjectRequirementDocument

    var body: some View {
        if Self.isSVG(document), let image = NSImage(data: Data(svgMarkup.utf8)) {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .scaledToFit()
                .frame(minWidth: 600, minHeight: 360)
                .background(.white, in: RoundedRectangle(cornerRadius: 10))
        } else if document.content.isEmpty {
            Text("暂无内容").foregroundStyle(.secondary)
        } else {
            MarkdownDocumentView(markdown: document.content)
        }
    }

    static func isSVG(_ document: ProjectRequirementDocument) -> Bool {
        document.type.lowercased().contains("svg")
            || document.format.lowercased() == "svg"
            || document.content.localizedCaseInsensitiveContains("<svg")
    }

    private var svgMarkup: String {
        guard let start = document.content.range(of: "<svg", options: .caseInsensitive),
              let end = document.content.range(of: "</svg>", options: [.caseInsensitive, .backwards]) else {
            return document.content
        }
        return String(document.content[start.lowerBound..<end.upperBound])
    }
}
