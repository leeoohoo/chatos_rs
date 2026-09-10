import ChatOSCore
import SwiftUI

struct StoryMediaBatchStartView: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: StoryStudioViewModel
    let project: StoryProject
    let models: [MediaGenerationModel]
    @State private var kind: StoryMediaBatch.Kind = .pipeline
    @State private var selected: Set<String> = []
    @State private var acceptsAutomaticVersions = false
    init(viewModel: StoryStudioViewModel, project: StoryProject, models: [MediaGenerationModel],
         initialKind: StoryMediaBatch.Kind = .pipeline) {
        self.viewModel = viewModel; self.project = project; self.models = models
        _kind = State(initialValue: initialKind)
    }
    private var candidates: [String] { StoryMediaBatch.candidates(project, kind: kind) }
    private var preview: StoryMediaBatch? { try? viewModel.previewMediaBatch(kind: kind, targets: candidates.filter(selected.contains), models: models) }
    private var previewError: String? {
        guard !selected.isEmpty else { return nil }
        do { _ = try viewModel.previewMediaBatch(kind: kind, targets: candidates.filter(selected.contains), models: models); return nil }
        catch { return error.localizedDescription }
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(appModel.localized("按已保存描述制作", english: "Produce from Saved Descriptions")).font(.title2.bold())
            Text(appModel.localized("直接按固定顺序执行，不调用文本 AI，也不消耗规划轮次。可单独制作，或一键执行所选分段的完整流程。", english: "Runs a fixed sequence without calling a text AI or consuming planning rounds. Produce one stage or run the complete pipeline for selected segments."))
                .font(.callout).foregroundStyle(.secondary)
            Picker(appModel.localized("执行范围", english: "Production Stage"), selection: $kind) {
                Text(appModel.localized("一键全流程", english: "Full Pipeline")).tag(StoryMediaBatch.Kind.pipeline)
                Text(appModel.localized("素材图片", english: "Asset Images")).tag(StoryMediaBatch.Kind.assets)
                Text(appModel.localized("分段首帧", english: "First Frames")).tag(StoryMediaBatch.Kind.frames)
                Text(appModel.localized("分段视频", english: "Videos")).tag(StoryMediaBatch.Kind.videos)
            }.pickerStyle(.segmented)
            HStack {
                Text(appModel.localized("只列出当前可执行、尚未完成的目标。", english: "Shows eligible unfinished targets only.")).font(.caption)
                Spacer()
                Button(appModel.localized("全选", english: "Select All")) { selected = Set(candidates) }
                Button(appModel.localized("清空", english: "Clear")) { selected = [] }
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(candidates, id: \.self) { id in
                        Toggle(isOn: Binding(get: { selected.contains(id) }, set: { on in if on { selected.insert(id) } else { selected.remove(id) } })) {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(kind == .assets ? project.resource(id: id)?.name ?? id : project.segments.first { $0.id == id }?.title ?? id)
                                Text(kind == .assets ? project.resource(id: id)?.prompt ?? "" : project.segments.first { $0.id == id }?.synopsis ?? "")
                                    .font(.caption).foregroundStyle(.secondary).lineLimit(2)
                            }
                        }.toggleStyle(.checkbox)
                    }
                    if candidates.isEmpty { Text(appModel.localized("没有就绪目标，请先完成文字计划或确认参考图片。", english: "No eligible targets. Complete the written plan or confirm reference images first.")).foregroundStyle(.secondary) }
                }.frame(maxWidth: .infinity, alignment: .leading)
            }.frame(height: 190)
            if kind == .pipeline {
                Toggle(appModel.localized("自动采用本次新生成的素材和首帧，继续生成视频", english: "Use newly generated assets and first frames automatically, then generate videos"), isOn: $acceptsAutomaticVersions)
                Text(appModel.localized("顺序：缺少的关联素材 → 缺少的首帧 → 所选视频。已有确认版本保持不变；已有未确认图片需先由你选定。", english: "Order: missing linked assets → missing first frames → selected videos. Existing confirmed versions are retained; select any existing unconfirmed images yourself first."))
                    .font(.caption).foregroundStyle(.secondary)
            }
            if let preview {
                let imageCount = preview.steps.filter { $0.kind != .videos }.count
                let videoCount = preview.steps.filter { $0.kind == .videos }.count
                Text(appModel.localized("本次将提交：\(imageCount) 张图片，\(videoCount) 段视频（每段 15 秒）。", english: "Will submit \(imageCount) images and \(videoCount) videos (15 seconds each)."))
                    .font(.headline)
                Text(models.filter { $0.id == project.models.imageModelID || $0.id == project.models.videoModelID }.map { "\($0.name) · \($0.modelName)" }.joined(separator: "\n")).font(.caption)
            }
            if let previewError { Text(previewError).font(.caption).foregroundStyle(.orange) }
            Text(appModel.localized("将调用所选图片/视频模型并可能计费。失败后停止后续提交；恢复仅处理未完成目标，已保存视频任务只查询原 ID。", english: "Calls your image/video models and may incur charges. Stops subsequent submissions on failure. Resume handles unfinished targets only; saved video tasks are queried by their original IDs."))
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button(appModel.localized("取消", english: "Cancel")) { dismiss() }
                Button(appModel.localized("确认并执行", english: "Confirm and Run")) {
                    if let preview { viewModel.startMediaBatch(preview); dismiss() }
                }.buttonStyle(.borderedProminent).disabled(preview == nil || viewModel.isBusy || (kind == .pipeline && !acceptsAutomaticVersions))
            }
        }.padding(24).frame(width: 720)
        .onAppear { selectDefaults() }
        .onChange(of: kind) { _, _ in selected = Set(candidates); acceptsAutomaticVersions = false }
    }
    private func selectDefaults() {
        let selectedSegments = viewModel.selectedSegments.intersection(Set(candidates))
        selected = selectedSegments.isEmpty ? Set(candidates) : selectedSegments
    }
}

struct StoryMediaBatchPanel: View {
    @EnvironmentObject private var appModel: AppModel
    @ObservedObject var viewModel: StoryStudioViewModel
    @State private var history = false
    @State private var resumeID: UUID?
    @State private var abandonID: UUID?
    var body: some View {
        if let latest = viewModel.projectMediaBatches.first {
            VStack(alignment: .leading, spacing: 8) {
                Text(appModel.localized("制作队列（无文本 AI）", english: "Production Queue (No Text AI)")).font(.headline)
                Text("\(latest.completedCount) / \(latest.steps.count) · " + status(latest)).font(.caption)
                if let error = latest.error { Text(error).font(.caption).foregroundStyle(.orange).lineLimit(3) }
                Button(appModel.localized("制作记录与恢复", english: "Production History & Recovery")) { history = true }
            }.padding(12).background(Color.blue.opacity(0.05), in: RoundedRectangle(cornerRadius: 10))
            .sheet(isPresented: $history) {
                VStack(alignment: .leading, spacing: 16) {
                    HStack {
                        Text(appModel.localized("制作记录", english: "Production History")).font(.title2.bold())
                        Spacer(); Button(appModel.localized("关闭", english: "Close")) { history = false }
                    }
                    ScrollView {
                        ForEach(viewModel.projectMediaBatches) { batch in
                            VStack(alignment: .leading, spacing: 8) {
                                Text("\(batch.completedCount) / \(batch.steps.count) · " + status(batch)).font(.headline)
                                Text(batch.consentAt, style: .date).font(.caption)
                                if let error = batch.error { Text(error).foregroundStyle(.orange) }
                                if !batch.finished {
                                    HStack {
                                        Button(appModel.localized("恢复批次", english: "Resume Batch")) { history = false; resumeID = batch.id }
                                        Button(appModel.localized("核对后结束批次", english: "End Batch After Verification")) { history = false; abandonID = batch.id }
                                    }.disabled(viewModel.isBusy)
                                }
                                DisclosureGroup(appModel.localized("最近 50 条记录", english: "Last 50 Events")) {
                                    ForEach(Array(batch.events.suffix(50))) { event in Text(event.detail).font(.caption).textSelection(.enabled) }
                                }
                            }.frame(maxWidth: .infinity, alignment: .leading).padding(12)
                        }
                    }
                }.padding(24).frame(width: 700, height: 540)
            }
            .confirmationDialog(appModel.localized("继续执行未完成目标？", english: "Continue Unfinished Targets?"), isPresented: Binding(get: { resumeID != nil }, set: { if !$0 { resumeID = nil } }), titleVisibility: .visible) {
                Button(appModel.localized("确认继续", english: "Confirm and Continue")) { if let resumeID { viewModel.resumeMediaBatch(resumeID) }; resumeID = nil }
            } message: { Text(appModel.localized("沿用已确认的范围、模型与版本策略，新的图片/视频提交可能计费。结果不明且没有任务 ID 时不会自动重发。", english: "Keeps the confirmed scope, models and version policy. New image/video submissions may incur charges. Uncertain submissions without a task ID are not retried automatically.")) }
            .confirmationDialog(appModel.localized("已核实原生成任务？", english: "Have You Verified the Original Tasks?"), isPresented: Binding(get: { abandonID != nil }, set: { if !$0 { abandonID = nil } }), titleVisibility: .visible) {
                Button(appModel.localized("已核对，结束旧批次", english: "Verified — End Old Batch")) { if let abandonID { viewModel.abandonMediaBatch(abandonID) }; abandonID = nil }
            } message: { Text(appModel.localized("图片提交结果不明时，请先核实失败或未创建；已生成的图片可手动导入。结束后可重新发起图片生成，可能再次计费。已有图片、视频和视频任务记录不会删除。", english: "For uncertain image submissions, verify failure or non-creation first; import successful results manually. Ending allows new image submissions that may incur another charge. Saved images, videos and video task records are retained.")) }
        }
    }
    private func status(_ batch: StoryMediaBatch) -> String {
        switch batch.status {
        case .completed: appModel.localized("完成", english: "Complete")
        case .abandoned: appModel.localized("已结束", english: "Ended")
        case .needsReview: appModel.localized("需核对 / 恢复", english: "Review / Resume Required")
        default: viewModel.isBusy ? appModel.localized("执行中", english: "Running") : appModel.localized("已暂停 / 中断", english: "Paused / Interrupted")
        }
    }
}
