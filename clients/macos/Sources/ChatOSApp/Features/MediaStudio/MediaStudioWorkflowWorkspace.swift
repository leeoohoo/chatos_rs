import AppKit
import ChatOSCore
import SwiftUI

struct MediaStudioWorkflowWorkspace: View {
    @ObservedObject var viewModel: MediaStudioViewModel
    @State private var confirmsWorkflowDeletion = false
    @State private var resultImagePreview: MediaStudioImagePreviewRequest?
    @State private var resultVideoPreview: MediaStudioVideoPreviewRequest?

    var body: some View {
        Group {
            if viewModel.selectedWorkflow == nil {
                workflowList
            } else {
                workflowEditor
            }
        }
        .overlay(alignment: .bottom) {
            if let message = viewModel.errorMessage {
                HStack(spacing: 10) {
                    Image(systemName: "exclamationmark.triangle.fill")
                    Text(message).font(.callout).textSelection(.enabled)
                    Button(action: viewModel.dismissError) {
                        Image(systemName: "xmark")
                    }
                    .buttonStyle(.plain)
                }
                .foregroundStyle(.red)
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
                .shadow(radius: 8, y: 3)
                .padding(.bottom, 14)
            }
        }
        .confirmationDialog(
            "确定删除这个工作流？",
            isPresented: $confirmsWorkflowDeletion,
            titleVisibility: .visible
        ) {
            Button("删除工作流", role: .destructive, action: viewModel.deleteSelectedWorkflow)
            Button("取消", role: .cancel) {}
        } message: {
            Text("工作流配置和已上传的节点图片会从本机删除；生成记录不受影响。")
        }
        .sheet(item: $resultImagePreview) { request in
            MediaStudioImagePreview(request: request)
        }
        .sheet(item: $resultVideoPreview) { request in
            MediaStudioVideoPreview(item: request)
        }
    }

    private var workflowList: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack(alignment: .bottom) {
                VStack(alignment: .leading, spacing: 6) {
                    Text("工作流")
                        .font(.system(size: 28, weight: .bold))
                    Text("把文本、图片和视频模型串成可重复运行的创作流程。")
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button(action: viewModel.createWorkflow) {
                    Label("新建工作流", systemImage: "plus")
                }
                .buttonStyle(.borderedProminent)
                .disabled(viewModel.isSavingWorkflow || viewModel.isLoadingModels)
            }

            if viewModel.isLoadingWorkflows {
                Spacer()
                ProgressView("正在读取工作流…")
                    .frame(maxWidth: .infinity)
                Spacer()
            } else if viewModel.workflows.isEmpty {
                ContentUnavailableView {
                    Label("还没有工作流", systemImage: "point.3.connected.trianglepath.dotted")
                } description: {
                    Text("新建一个流程，依次连接文本、图片或视频节点。")
                } actions: {
                    Button("新建工作流", action: viewModel.createWorkflow)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 260), spacing: 16)],
                        spacing: 16
                    ) {
                        ForEach(viewModel.workflows) { workflow in
                            workflowCard(workflow)
                        }
                    }
                    .padding(.bottom, 20)
                }
            }
        }
        .padding(28)
    }

    private func workflowCard(_ workflow: MediaStudioWorkflow) -> some View {
        Button { viewModel.openWorkflow(workflow) } label: {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    Image(systemName: "point.3.connected.trianglepath.dotted")
                        .font(.system(size: 18, weight: .semibold))
                        .foregroundStyle(.purple)
                        .frame(width: 38, height: 38)
                        .background(.purple.opacity(0.1), in: RoundedRectangle(cornerRadius: 10))
                    Spacer()
                    Text(workflow.updatedAt, style: .relative)
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
                Text(workflow.name)
                    .font(.system(size: 17, weight: .semibold))
                    .lineLimit(1)
                HStack(spacing: 7) {
                    ForEach(workflow.nodes.prefix(6)) { node in
                        Image(systemName: icon(for: node.kind))
                            .font(.caption)
                            .foregroundStyle(color(for: node.kind))
                    }
                    if workflow.nodes.count > 6 { Text("+") }
                    Spacer()
                    Text("\(workflow.nodes.count) 个节点")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(18)
            .frame(maxWidth: .infinity, minHeight: 155, alignment: .leading)
            .background(.background.opacity(0.92), in: RoundedRectangle(cornerRadius: 16))
            .overlay {
                RoundedRectangle(cornerRadius: 16)
                    .stroke(.quaternary, lineWidth: 1)
            }
        }
        .buttonStyle(.plain)
    }

    private var workflowEditor: some View {
        VStack(spacing: 0) {
            editorToolbar
            Divider()
            HStack(spacing: 0) {
                flowCanvas
                Divider()
                inspector
                    .frame(width: 330)
            }
        }
    }

    private var editorToolbar: some View {
        HStack(spacing: 12) {
            Button(action: viewModel.closeWorkflow) {
                Image(systemName: "chevron.left")
            }
            .disabled(viewModel.isRunningWorkflow || viewModel.isSavingWorkflow)

            TextField("工作流名称", text: workflowName)
                .textFieldStyle(.plain)
                .font(.system(size: 17, weight: .semibold))
                .frame(maxWidth: 360)
                .disabled(viewModel.isRunningWorkflow || viewModel.isSavingWorkflow)
            Spacer()
            if viewModel.isSavingWorkflow { ProgressView().controlSize(.small) }
            else if viewModel.workflowSavedAt != nil {
                Label("已保存", systemImage: "checkmark.circle.fill")
                    .font(.caption)
                    .foregroundStyle(.green)
            }
            Button("保存", action: viewModel.saveSelectedWorkflow)
                .disabled(viewModel.isSavingWorkflow || viewModel.isRunningWorkflow)
            if viewModel.isRunningWorkflow {
                Button("停止", action: viewModel.cancelWorkflowRun)
                    .tint(.red)
            } else {
                Button(action: viewModel.runSelectedWorkflow) {
                    Label("运行工作流", systemImage: "play.fill")
                }
                .buttonStyle(.borderedProminent)
                .disabled(viewModel.isLoadingModels || viewModel.isLoadingHistory)
            }
        }
        .padding(.horizontal, 18)
        .frame(height: 58)
        .background(.bar)
    }

    private var flowCanvas: some View {
        ScrollView([.horizontal, .vertical]) {
            HStack(spacing: 12) {
                flowStart
                arrow
                if let workflow = viewModel.selectedWorkflow {
                    ForEach(Array(workflow.nodes.enumerated()), id: \.element.id) { index, node in
                        nodeCard(node, number: index + 1)
                        if index < workflow.nodes.count - 1 { arrow }
                    }
                    if workflow.nodes.last?.kind != .video,
                       workflow.nodes.count < MediaStudioWorkflowValidator.maximumNodeCount {
                        arrow
                        addNodeMenu
                    }
                }
            }
            .padding(40)
            .frame(minWidth: 760, minHeight: 420, alignment: .leading)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.38))
    }

    private var flowStart: some View {
        Label("开始", systemImage: "play.fill")
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(.background, in: Capsule())
            .overlay { Capsule().stroke(.secondary.opacity(0.25)) }
    }

    private func nodeCard(_ node: MediaStudioWorkflowNode, number: Int) -> some View {
        Button { viewModel.selectedWorkflowNodeID = node.id } label: {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Image(systemName: icon(for: node.kind))
                        .foregroundStyle(color(for: node.kind))
                    Text("\(number). \(title(for: node.kind))")
                        .font(.system(size: 14, weight: .semibold))
                    Spacer()
                    stateBadge(viewModel.workflowNodeStates[node.id])
                }
                Text(modelName(node.modelConfigID))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Text(node.prompt)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
                Spacer()
                Label("\(node.referenceImages.count) 张节点图片", systemImage: "photo.on.rectangle")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            .flowNodeStyle(
                selected: viewModel.selectedWorkflowNodeID == node.id,
                color: color(for: node.kind)
            )
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private func stateBadge(_ state: MediaStudioViewModel.WorkflowNodeState?) -> some View {
        switch state {
        case .running:
            ProgressView().controlSize(.mini)
        case .completed:
            Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
        case .failed:
            Image(systemName: "exclamationmark.circle.fill").foregroundStyle(.red)
        case nil:
            EmptyView()
        }
    }

    private var arrow: some View {
        HStack(spacing: 0) {
            Rectangle().fill(.secondary.opacity(0.45)).frame(width: 24, height: 1)
            Image(systemName: "chevron.right")
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(.secondary)
        }
    }

    private var addNodeMenu: some View {
        Menu {
            Button("文本节点") { viewModel.addWorkflowNode(.text) }
            Button("图片节点") { viewModel.addWorkflowNode(.image) }
            Button("视频节点") { viewModel.addWorkflowNode(.video) }
        } label: {
            VStack(spacing: 8) {
                Image(systemName: "plus")
                    .font(.system(size: 20, weight: .medium))
                Text("添加节点").font(.caption)
            }
            .frame(width: 110, height: 92)
            .background(.background.opacity(0.8), in: RoundedRectangle(cornerRadius: 14))
            .overlay { RoundedRectangle(cornerRadius: 14).strokeBorder(style: .init(dash: [5])) }
        }
        .menuStyle(.borderlessButton)
        .disabled(viewModel.isRunningWorkflow)
    }

    @ViewBuilder
    private var inspector: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                if let node = viewModel.selectedWorkflowNode {
                    nodeInspector(node)
                }
            }
            .padding(20)
        }
        .disabled(viewModel.isRunningWorkflow || viewModel.isSavingWorkflow)
        .background(.background.opacity(0.75))
    }

    private func nodeInspector(_ node: MediaStudioWorkflowNode) -> some View {
        Group {
            inspectorTitle(title(for: node.kind), subtitle: "配置模型、提示词和节点图片")
            nodeResult(viewModel.workflowNodeStates[node.id])
            fieldLabel("节点类型")
            Picker("", selection: Binding(
                get: { node.kind },
                set: viewModel.changeSelectedWorkflowNodeKind
            )) {
                Text("文本").tag(MediaStudioWorkflowNodeKind.text)
                Text("图片").tag(MediaStudioWorkflowNodeKind.image)
                Text("视频").tag(MediaStudioWorkflowNodeKind.video)
            }
            .labelsHidden()
            .pickerStyle(.segmented)

            fieldLabel("模型")
            Picker("模型", selection: nodeModelID) {
                ForEach(modelOptions(for: node.kind)) { model in
                    Text(model.name).tag(model.id)
                }
            }
            .labelsHidden()

            fieldLabel("节点提示词")
            TextEditor(text: nodePrompt)
                .frame(minHeight: 130)
                .padding(7)
                .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 8))

            if node.kind == .image {
                fieldLabel("图片尺寸")
                Picker("图片尺寸", selection: imageSize) {
                    ForEach(["auto", "1024x1024", "1536x1024", "1024x1536"], id: \.self) { Text($0) }
                }
                .labelsHidden()
            } else if node.kind == .video {
                videoOptions
            }

            imageSection(references: node.referenceImages, nodeID: node.id)
            HStack {
                Button { viewModel.moveSelectedWorkflowNode(by: -1) } label: {
                    Image(systemName: "arrow.left")
                }
                Button { viewModel.moveSelectedWorkflowNode(by: 1) } label: {
                    Image(systemName: "arrow.right")
                }
                Spacer()
                Button("删除节点", role: .destructive, action: viewModel.removeSelectedWorkflowNode)
            }
            .disabled(viewModel.isRunningWorkflow)
            Divider()
            Button("删除工作流", role: .destructive) {
                confirmsWorkflowDeletion = true
            }
            .disabled(viewModel.isRunningWorkflow)
        }
    }

    @ViewBuilder
    private func nodeResult(_ state: MediaStudioViewModel.WorkflowNodeState?) -> some View {
        switch state {
        case let .running(progress):
            resultContainer {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text(progress ?? "正在运行…")
                }
            }
        case let .completed(.text(text)):
            resultContainer {
                ScrollView {
                    Text(text)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(maxHeight: 220)
            }
        case let .completed(.images(item)):
            resultContainer {
                ForEach(Array(item.images.enumerated()), id: \.element.id) { index, asset in
                    Button {
                        resultImagePreview = .init(images: item.images, selectedIndex: index)
                    } label: {
                        GeneratedMediaAssetView(asset: asset, compact: true)
                    }
                    .buttonStyle(.plain)
                    .help("放大查看图片")
                }
            }
        case let .completed(.video(item)):
            resultContainer {
                Button {
                    resultVideoPreview = .init(item)
                } label: {
                    Label("播放生成视频", systemImage: "play.rectangle.fill")
                }
            }
        case let .failed(message):
            resultContainer {
                Label(message, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
                    .textSelection(.enabled)
            }
        case nil:
            EmptyView()
        }
    }

    private func resultContainer<Content: View>(
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            fieldLabel("本次运行结果")
            content()
        }
        .padding(12)
        .background(.quaternary.opacity(0.24), in: RoundedRectangle(cornerRadius: 10))
    }

    private var videoOptions: some View {
        Group {
            fieldLabel("视频尺寸")
            TextField("1280x720", text: videoSize)
            fieldLabel("画面比例")
            Picker("比例", selection: videoRatio) {
                ForEach(["16:9", "9:16", "1:1"], id: \.self) { Text($0) }
            }
            .labelsHidden()
            fieldLabel("时长")
            Stepper("\(viewModel.selectedWorkflowNode?.videoSeconds ?? 4) 秒", value: videoSeconds, in: 1...120)
        }
    }

    private func imageSection(
        references: [MediaStudioWorkflowImageReference],
        nodeID: UUID
    ) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                fieldLabel("上传图片")
                Spacer()
                Button {
                    chooseImage(nodeID: nodeID)
                } label: {
                    Label("添加", systemImage: "plus")
                }
                .disabled(viewModel.isSavingWorkflow || viewModel.isRunningWorkflow)
            }
            if references.isEmpty {
                Text("可上传图片作为这个节点的额外输入。")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(references) { reference in
                    HStack {
                        Image(systemName: "photo")
                            .foregroundStyle(.purple)
                        Text(reference.name).lineLimit(1)
                        Spacer()
                        Button {
                            viewModel.removeWorkflowImage(reference.id, nodeID: nodeID)
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.secondary)
                        }
                        .buttonStyle(.plain)
                    }
                    .padding(9)
                    .background(.quaternary.opacity(0.3), in: RoundedRectangle(cornerRadius: 8))
                }
            }
        }
    }

    private func inspectorTitle(_ title: String, subtitle: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.title3.weight(.semibold))
            Text(subtitle).font(.caption).foregroundStyle(.secondary)
        }
    }

    private func fieldLabel(_ value: String) -> some View {
        Text(value).font(.caption.weight(.medium)).foregroundStyle(.secondary)
    }

    private func chooseImage(nodeID: UUID) {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        viewModel.importWorkflowImage(from: url, nodeID: nodeID)
    }

    private func modelOptions(for kind: MediaStudioWorkflowNodeKind) -> [MediaGenerationModel] {
        switch kind {
        case .text: viewModel.textModels
        case .image: viewModel.imageModels
        case .video: viewModel.videoModels
        }
    }

    private func modelName(_ id: String) -> String {
        viewModel.models.first(where: { $0.id == id })?.name ?? "未选择模型"
    }

    private func icon(for kind: MediaStudioWorkflowNodeKind) -> String {
        switch kind {
        case .text: "text.alignleft"
        case .image: "photo"
        case .video: "video"
        }
    }

    private func title(for kind: MediaStudioWorkflowNodeKind) -> String {
        switch kind {
        case .text: "文本节点"
        case .image: "图片节点"
        case .video: "视频节点"
        }
    }

    private func color(for kind: MediaStudioWorkflowNodeKind) -> Color {
        switch kind {
        case .text: .blue
        case .image: .purple
        case .video: .orange
        }
    }

    private var workflowName: Binding<String> {
        .init(
            get: { viewModel.selectedWorkflow?.name ?? "" },
            set: { value in viewModel.editSelectedWorkflow { $0.name = value } }
        )
    }

    private var nodePrompt: Binding<String> {
        .init(
            get: { viewModel.selectedWorkflowNode?.prompt ?? "" },
            set: { value in viewModel.editSelectedNode { $0.prompt = value } }
        )
    }

    private var nodeModelID: Binding<String> {
        .init(
            get: { viewModel.selectedWorkflowNode?.modelConfigID ?? "" },
            set: { value in viewModel.editSelectedNode { $0.modelConfigID = value } }
        )
    }

    private var imageSize: Binding<String> {
        .init(
            get: { viewModel.selectedWorkflowNode?.imageSize ?? "1024x1024" },
            set: { value in viewModel.editSelectedNode { $0.imageSize = value } }
        )
    }

    private var videoSize: Binding<String> {
        .init(
            get: { viewModel.selectedWorkflowNode?.videoSize ?? "1280x720" },
            set: { value in viewModel.editSelectedNode { $0.videoSize = value } }
        )
    }

    private var videoRatio: Binding<String> {
        .init(
            get: { viewModel.selectedWorkflowNode?.videoRatio ?? "16:9" },
            set: { value in viewModel.editSelectedNode { $0.videoRatio = value } }
        )
    }

    private var videoSeconds: Binding<Int> {
        .init(
            get: { viewModel.selectedWorkflowNode?.videoSeconds ?? 4 },
            set: { value in viewModel.editSelectedNode { $0.videoSeconds = value } }
        )
    }
}

private extension View {
    func flowNodeStyle(selected: Bool, color: Color) -> some View {
        frame(width: 210, height: 150, alignment: .leading)
            .padding(16)
            .background(.background, in: RoundedRectangle(cornerRadius: 16))
            .overlay {
                RoundedRectangle(cornerRadius: 16)
                    .stroke(selected ? color : .secondary.opacity(0.25), lineWidth: selected ? 2 : 1)
            }
            .shadow(color: selected ? color.opacity(0.14) : .black.opacity(0.04), radius: 10, y: 4)
    }
}
