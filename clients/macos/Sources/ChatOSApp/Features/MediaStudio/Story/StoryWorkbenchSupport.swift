import AppKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

extension StoryWorkbenchView {
    var batchBar: some View {
        let activeVideos = viewModel.activeVideoGenerationCount(projectID: project.id)
        return HStack(spacing: 16) {
            ZStack {
                Circle().fill(Color.blue.opacity(0.12))
                Image(systemName: "film.stack.fill").foregroundStyle(.blue)
            }.frame(width: 38, height: 38)
            VStack(alignment: .leading, spacing: 5) {
                Text("\(project.completedCount) / \(project.segments.count) " + appModel.localized("段已完成", english: "segments complete")).font(.callout.bold())
                Text(viewModel.isBusy
                     ? viewModel.operation
                     : activeVideos > 0
                        ? appModel.localized("\(activeVideos) 段正在并行生成；仍可继续提交其它就绪分段",
                                             english: "\(activeVideos) running in parallel; more ready segments can still be submitted")
                        : appModel.localized("先确认素材与首帧，再批量生成视频", english: "Confirm assets and first frames before generating videos"))
                    .font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if let playlistGroup {
                Button { playlistPreview = playlistGroup } label: {
                    Label(playlistGroup.isComplete
                          ? appModel.localized("全剧连播", english: "Play Full Story")
                          : appModel.localized("连播已完成分段", english: "Play Completed Segments"),
                          systemImage: "play.fill")
                }
                .buttonStyle(.borderedProminent)
                .tint(.indigo)
            }
            if viewModel.isBusy {
                ProgressView().controlSize(.small)
                Button(viewModel.pauseRequested ? appModel.localized("将在当前步骤结束后暂停", english: "Pausing after this step") : appModel.localized("暂停后续任务", english: "Pause Remaining Tasks")) { viewModel.requestPause() }
                    .disabled(viewModel.pauseRequested)
            } else {
                if activeVideos > 0 { ProgressView().controlSize(.small) }
                Text("\(selectedReady.count) " + appModel.localized("段已选", english: "selected") + " · \(selectedReadySeconds)s").font(.callout)
                Button(appModel.localized("选择全部就绪段", english: "Select Ready Segments")) {
                    viewModel.selectedSegments = Set(project.segments.filter(\.isReady).map(\.id))
                }.disabled(viewModel.isBusy || isAgentDraftVisible)
                Button(appModel.localized("批量生成所选视频", english: "Generate Selected Videos")) {
                    mediaBatchKind = .videos
                    showsMediaBatch = true
                }
                    .buttonStyle(.borderedProminent).tint(.blue)
                    .disabled(viewModel.isBusy || isAgentDraftVisible || selectedReady.isEmpty)
            }
        }
        .padding(.horizontal, 24).padding(.vertical, 12)
        .background(.ultraThinMaterial)
        .overlay(alignment: .top) { Divider().opacity(0.65) }
    }

    func sectionHeading(icon: String, color: Color, title: String, subtitle: String) -> some View {
        HStack(alignment: .center, spacing: 13) {
            ZStack {
                RoundedRectangle(cornerRadius: 11, style: .continuous).fill(color.opacity(0.11))
                Image(systemName: icon).font(.system(size: 19, weight: .semibold)).foregroundStyle(color)
            }.frame(width: 44, height: 44)
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.title2.bold())
                Text(subtitle).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    func readinessRow(_ title: String, ready: Bool) -> some View {
        HStack(spacing: 9) {
            Image(systemName: ready ? "checkmark.circle.fill" : "circle")
                .foregroundStyle(ready ? Color.green : .secondary)
            Text(title).font(.callout)
            Spacer()
            Text(ready ? appModel.localized("完成", english: "Ready") : appModel.localized("待完成", english: "Pending"))
                .font(.caption2).foregroundStyle(.secondary)
        }
    }

    func ratioChoice(_ option: String) -> some View {
        let dimensions: CGSize = if option == "9:16" {
            .init(width: 15, height: 25)
        } else if option == "1:1" {
            .init(width: 22, height: 22)
        } else {
            .init(width: 28, height: 17)
        }
        return VStack(spacing: 7) {
            RoundedRectangle(cornerRadius: 3)
                .stroke(ratio == option ? Color.pink : Color.secondary.opacity(0.55), lineWidth: ratio == option ? 2 : 1)
                .frame(width: dimensions.width, height: dimensions.height)
                .frame(height: 27)
            Text(option).font(.caption.bold().monospacedDigit())
        }
        .frame(maxWidth: .infinity).padding(.vertical, 10)
        .foregroundStyle(ratio == option ? Color.pink : .secondary)
        .background(ratio == option ? Color.pink.opacity(0.09) : Color.primary.opacity(0.025),
                    in: RoundedRectangle(cornerRadius: 9, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 9, style: .continuous)
            .stroke(ratio == option ? Color.pink.opacity(0.28) : Color.primary.opacity(0.05)))
    }

    func portraitMetric(_ value: Int, _ title: String, color: Color) -> some View {
        VStack(spacing: 2) {
            Text("\(value)").font(.headline.monospacedDigit()).foregroundStyle(color)
            Text(title).font(.caption2).foregroundStyle(.secondary)
        }.frame(minWidth: 42)
    }

    func assetKindLabel(_ asset: StoryResource) -> String {
        switch asset.kind {
        case .character: appModel.localized("人物", english: "CHARACTER")
        case .scene: appModel.localized("场景", english: "SCENE")
        case .prop: appModel.localized("道具", english: "PROP")
        }
    }

    func assetKindColor(_ asset: StoryResource) -> Color {
        switch asset.kind {
        case .character: .purple
        case .scene: .orange
        case .prop: .green
        }
    }

    func hasConfirmedAssets(_ segment: StorySegment) -> Bool {
        segment.resourceIDs.allSatisfy { project.resource(id: $0)?.confirmedImage != nil }
    }
    func frameGenerationTitle(_ collection: StoryImageCollection, role: StoryFrameRole) -> String {
        if !collection.images.isEmpty {
            return role == .first
                ? appModel.localized("重新生成首帧", english: "Regenerate First Frame")
                : appModel.localized("重新生成尾帧", english: "Regenerate Last Frame")
        }
        return role == .first
            ? appModel.localized("选择素材并生成首帧", english: "Choose Assets and Generate First Frame")
            : appModel.localized("选择素材并生成尾帧", english: "Choose Assets and Generate Last Frame")
    }
    func status(_ segment: StorySegment) -> String {
        if segment.video != nil { return appModel.localized("视频已完成", english: "Video Complete") }
        if viewModel.isGeneratingVideo(segment.id, projectID: project.id) {
            return appModel.localized("正在生成视频", english: "Generating Video")
        }
        if segment.attempt != nil { return appModel.localized("已提交 · 查询原任务", english: "Submitted · Check Existing Task") }
        if viewModel.isGeneratingFrame(segment.id, role: .first, projectID: project.id) {
            return appModel.localized("正在生成首帧 · 可点开查看", english: "Generating First Frame · Click to View")
        }
        if viewModel.isGeneratingFrame(segment.id, role: .last, projectID: project.id) {
            return appModel.localized("正在生成尾帧 · 可点开查看", english: "Generating Last Frame · Click to View")
        }
        if segment.detail == nil { return appModel.localized("待细化镜头计划", english: "Needs Shot Plan") }
        if segment.firstFrame == nil {
            return segment.firstFrames.images.isEmpty
                ? appModel.localized("待生成首帧", english: "Needs First Frame")
                : appModel.localized("首帧已生成 · 待确认", english: "First Frame Generated · Needs Confirmation")
        }
        return appModel.localized("就绪 · 可以生成视频", english: "Ready to Generate")
    }
    func time(_ seconds: Int) -> String { String(format: "%02d:%02d", seconds / 60, seconds % 60) }
    func rememberDraft() { viewModel.rememberSourceDraft(projectID: project.id, source: source, style: style, ratio: ratio) }
    func confirmPlanning(_ stage: StoryAgentRun.Stage, targets: [String]) {
        planningConfirmation = .init(stage: stage, targets: targets, project: project)
    }
}
