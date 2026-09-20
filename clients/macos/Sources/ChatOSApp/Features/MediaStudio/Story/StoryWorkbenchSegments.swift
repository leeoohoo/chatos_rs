import AppKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

extension StoryWorkbenchView {
    var segmentsPanel: some View {
        VStack(alignment: .leading, spacing: 16) {
            if !viewModel.projectMediaBatches.isEmpty {
                StoryMediaBatchPanel(viewModel: viewModel)
            }
            HStack(alignment: .top, spacing: 16) {
                overview.frame(width: 410)
                detailPanel
                    .frame(minWidth: 560, maxWidth: .infinity, maxHeight: .infinity,
                           alignment: .topLeading)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .onAppear { selectFirstSegmentIfNeeded(project.segments.map(\.id)) }
        .onChange(of: project.segments.map(\.id)) { _, ids in selectFirstSegmentIfNeeded(ids) }
    }

    var overview: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                VStack(alignment: .leading, spacing: 6) {
                    Text(appModel.localized("全剧时间线", english: "Story Timeline")).font(.title3.bold())
                    Text("\(project.segments.count) " + appModel.localized("个分段", english: "segments") + " · \(project.totalSeconds)s")
                        .font(.callout.monospacedDigit()).foregroundStyle(.blue)
                }
                Spacer()
                Menu(appModel.localized("计划操作", english: "Plan Actions")) {
                    Button(appModel.localized("批量制作 / 一键执行", english: "Batch Production / Full Pipeline")) {
                        mediaBatchKind = .pipeline; showsMediaBatch = true
                    }
                    Button(appModel.localized("逐段细化未完成计划", english: "Refine Unfinished Plans")) {
                        confirmPlanning(.refine, targets: project.segments.filter { $0.detail == nil && $0.attempt == nil && $0.video == nil }.map(\.id))
                    }.disabled(project.segments.isEmpty)
                    Button(appModel.localized("添加剧情分段", english: "Add Story Segment")) { viewModel.addSegment() }
                        .disabled(project.source.isEmpty || project.hasUnresolvedJobs || project.completedCount > 0)
                }.disabled(editingLocked)
            }
            if project.segments.isEmpty {
                ContentUnavailableView {
                    Label(appModel.localized("先规划整个故事", english: "Plan the Entire Story"), systemImage: "list.bullet.rectangle")
                } description: {
                    Text(appModel.localized("请先到“剧情原文”保存完整故事并规划，之后这里会展示剧情段与独立转场段。", english: "Save and plan the complete story first; story segments and independent transitions will appear here."))
                }.frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(spacing: 9) {
                        ForEach(Array(project.segments.enumerated()), id: \.element.id) { index, segment in
                            segmentRow(segment, index: index)
                        }
                    }
                }
                Text(appModel.localized("每段单独生成，完成后可按剧情顺序连续播放。", english: "Segments generate separately, then play continuously in story order."))
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .padding(18)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .storySurface(tint: .blue)
    }

    func segmentRow(_ segment: StorySegment, index: Int) -> some View {
        let start = project.segments.prefix(index).reduce(0) { $0 + $1.seconds }
        let tint: Color = segment.kind == .transition ? .purple : .blue
        return HStack(spacing: 10) {
            Toggle("", isOn: Binding(get: { viewModel.selectedSegments.contains(segment.id) }, set: { on in
                if on { viewModel.selectedSegments.insert(segment.id) } else { viewModel.selectedSegments.remove(segment.id) }
            })).labelsHidden().toggleStyle(.checkbox)
                .disabled(!segment.isReady || viewModel.isBusy || isAgentDraftVisible)
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Text(String(format: "%02d", index + 1))
                        .font(.caption.bold().monospacedDigit()).foregroundStyle(.white)
                        .frame(width: 27, height: 27).background(tint, in: Circle())
                    Text(segment.title).fontWeight(.semibold).lineLimit(1)
                    if segment.kind == .transition {
                        Text(appModel.localized("转场", english: "Transition"))
                            .font(.caption2.bold()).foregroundStyle(.purple)
                            .padding(.horizontal, 7).padding(.vertical, 3)
                            .background(Color.purple.opacity(0.1), in: Capsule())
                    }
                    Spacer(minLength: 4)
                    Text("\(time(start))–\(time(start + segment.seconds))").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                }
                Text(segment.synopsis).font(.caption).lineLimit(2).foregroundStyle(.secondary)
                HStack {
                    Text(status(segment)).font(.caption2).foregroundStyle(segment.video == nil ? Color.blue : .green)
                    Spacer()
                    Text("\(segment.seconds)s").font(.caption2).foregroundStyle(.secondary)
                }
            }.contentShape(Rectangle()).onTapGesture { viewModel.selectedSegmentID = segment.id }
        }.padding(12)
        .background(viewModel.selectedSegmentID == segment.id ? Color.blue.opacity(0.09) : Color.primary.opacity(0.025),
                    in: RoundedRectangle(cornerRadius: 11, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 11, style: .continuous)
            .stroke(viewModel.selectedSegmentID == segment.id ? Color.blue.opacity(0.55) : Color.primary.opacity(0.045)))
        .contextMenu {
            Button(appModel.localized("编辑分段", english: "Edit Segment")) { editSegment = segment }.disabled(editingLocked || segment.attempt != nil)
            Button(appModel.localized("上移（重新细化衔接）", english: "Move Up and Re-plan")) { viewModel.moveSegment(segment.id, offset: -1) }
                .disabled(editingLocked || index == 0 || project.hasUnresolvedJobs || project.completedCount > 0)
            Button(appModel.localized("下移（重新细化衔接）", english: "Move Down and Re-plan")) { viewModel.moveSegment(segment.id, offset: 1) }
                .disabled(editingLocked || index + 1 == project.segments.count || project.hasUnresolvedJobs || project.completedCount > 0)
            Button(appModel.localized("删除分段", english: "Remove Segment"), role: .destructive) { segmentToDelete = segment.id }
                .disabled(editingLocked || project.hasUnresolvedJobs || project.completedCount > 0)
        }
    }

}
