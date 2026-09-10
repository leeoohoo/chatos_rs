import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

struct StoryImageTarget: Identifiable {
    let id = UUID()
    var assetID: String?
    var segmentID: String?
}

struct StoryImagePicker: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: StoryStudioViewModel
    @ObservedObject var mediaStudio: MediaStudioViewModel
    let project: StoryProject
    let target: StoryImageTarget
    @State private var showsUpload = false
    @State private var preview: MediaStudioImagePreviewRequest?
    @State private var assetPrompt = ""
    @State private var confirmsImageRetry = false
    private var current: StoryProject { viewModel.projects.first { $0.id == project.id } ?? project }
    private var asset: StoryResource? { current.resources.first { $0.id == target.assetID } }
    private var segment: StorySegment? { current.segments.first { $0.id == target.segmentID } }
    private var images: [StoryImage] { asset?.images ?? segment?.firstFrames.images ?? [] }
    private var confirmedID: UUID? { asset?.confirmedImageID ?? segment?.confirmedFrameID }
    private var hasUnresolvedImageSubmission: Bool {
        asset?.imageGenerationAttemptID != nil || segment?.imageGenerationAttemptID != nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text(asset?.name ?? appModel.localized("分段首帧", english: "Segment First Frame")).font(.title2.bold())
                Spacer()
                Button(appModel.localized("关闭", english: "Close")) { dismiss() }.keyboardShortcut(.cancelAction)
            }
            if let asset {
                if let profile = asset.characterProfile {
                    DisclosureGroup(appModel.localized("查看人物文字画像（不是图片）", english: "View Written Character Profile (Not an Image)")) {
                        ScrollView { StoryCharacterProfileView(profile: profile).frame(maxWidth: .infinity, alignment: .leading) }.frame(maxHeight: 200)
                    }
                }
                if let profile = asset.sceneProfile {
                    DisclosureGroup(appModel.localized("查看场景文字画像（不是图片）", english: "View Written Scene Profile (Not an Image)")) {
                        ScrollView { StorySceneProfileView(profile: profile).frame(maxWidth: .infinity, alignment: .leading) }.frame(maxHeight: 200)
                    }
                }
                TextEditor(text: $assetPrompt).frame(height: 75)
                Button(appModel.localized("保存素材描述", english: "Save Asset Description")) {
                    viewModel.updateAssetPrompt(asset.id, prompt: assetPrompt)
                }.disabled(viewModel.isBusy || assetPrompt == asset.prompt || assetPrompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                           || assetPrompt.count > 4_000 || current.hasUnresolvedJobs)
            }
            if let error = viewModel.errorMessage { Text(error).font(.caption).foregroundStyle(.red) }
            if hasUnresolvedImageSubmission {
                VStack(alignment: .leading, spacing: 6) {
                    Text(appModel.localized("上次图片提交结果尚未核实，为避免重复扣费，当前禁止再次提交。", english: "The previous image submission is unresolved, so resubmission is blocked to avoid duplicate charges."))
                        .font(.caption).foregroundStyle(.orange)
                    Button(appModel.localized("核对后允许重试…", english: "Allow Retry after Verification…")) { confirmsImageRetry = true }
                        .disabled(viewModel.isBusy)
                }
            }
            HStack {
                if let id = target.assetID {
                    Button(appModel.localized("生成素材图片", english: "Generate Asset Image")) { viewModel.generateAsset(id) }
                        .buttonStyle(.borderedProminent).tint(.purple)
                        .disabled(viewModel.isBusy || hasUnresolvedImageSubmission || assetPrompt != asset?.prompt)
                }
                Button(appModel.localized("本机上传", english: "Upload Image")) { showsUpload = true }.disabled(viewModel.isBusy)
                if viewModel.isBusy { ProgressView().controlSize(.small); Text(viewModel.operation).font(.caption) }
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text(appModel.localized("素材版本 · 选择后确认", english: "Image Versions · Confirm One")).font(.headline)
                    if images.isEmpty {
                        Text(appModel.localized("还没有图片，生成、上传或选择一张已有图片。", english: "Generate, upload or choose an existing image to begin."))
                            .foregroundStyle(.secondary)
                    }
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 175))], spacing: 14) {
                        ForEach(images) { image in
                            VStack(spacing: 8) {
                                Button {
                                    if let asset = viewModel.mediaAsset(image, projectID: current.id) { preview = .init(images: [asset]) }
                                } label: {
                                    StoryThumbnail(asset: viewModel.mediaAsset(image, projectID: current.id)).frame(height: 110).clipped()
                                }.buttonStyle(.plain)
                                Button(confirmedID == image.id ? appModel.localized("已确认使用", english: "Confirmed") : appModel.localized("确认使用此图", english: "Use This Image")) {
                                    viewModel.confirmImage(image, assetID: target.assetID, segmentID: target.segmentID)
                                }.disabled(viewModel.isBusy || confirmedID == image.id)
                            }.padding(8).background(Color.primary.opacity(0.03), in: RoundedRectangle(cornerRadius: 8))
                        }
                    }
                    Divider()
                    Text(appModel.localized("从图片生成记录选择", english: "Choose from Image History")).font(.headline)
                    if mediaStudio.history.isEmpty {
                        Text(appModel.localized("还没有图片生成记录，本机上传仍可使用。", english: "No generated images yet. You can still upload an image."))
                            .font(.callout).foregroundStyle(.secondary)
                    }
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 175))], spacing: 14) {
                        ForEach(mediaStudio.history) { record in
                            ForEach(record.images) { image in
                                Button { viewModel.importImage(image, assetID: target.assetID, segmentID: target.segmentID) } label: {
                                    VStack {
                                        StoryThumbnail(asset: image).frame(height: 110).clipped()
                                        Text(record.prompt).font(.caption).lineLimit(2)
                                    }
                                }.buttonStyle(.plain).disabled(viewModel.isBusy)
                            }
                        }
                    }
                }
            }
            Text(appModel.localized("旧版本会保留；确认角色或场景新版本后，相关未生成分段需要重新确认首帧。", english: "Old versions are retained. Changing a shared asset requires affected first frames to be confirmed again."))
                .font(.caption).foregroundStyle(.secondary)
        }.padding(22).frame(width: 720, height: 680)
        .onAppear { assetPrompt = asset?.prompt ?? "" }
        .fileImporter(isPresented: $showsUpload, allowedContentTypes: [.image]) { result in
            switch result {
            case .success(let url): viewModel.uploadImage(url, assetID: target.assetID, segmentID: target.segmentID)
            case .failure(let error): mediaStudio.reportInputImageError(error)
            }
        }
        .sheet(item: $preview) { request in
            MediaStudioImagePreview(request: request) { mediaStudio.useGeneratedImageForVideo($0) }.environmentObject(appModel)
        }
        .confirmationDialog(appModel.localized("确认已核对图片任务？", english: "Have You Verified the Image Task?"),
                            isPresented: $confirmsImageRetry, titleVisibility: .visible) {
            Button(appModel.localized("确认未生成，允许重试", english: "Confirmed No Result — Allow Retry")) {
                viewModel.allowImageRetryAfterVerification(assetID: target.assetID, segmentID: target.segmentID)
            }
        } message: {
            Text(appModel.localized("只有确认服务商没有生成可用结果后才应解锁。下一次点击生成会再次提交，并可能再次计费。", english: "Unlock only after confirming the provider produced no usable result. The next Generate click submits again and may incur another charge."))
        }
    }
}

struct StorySegmentEditor: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    let project: StoryProject
    @State private var segment: StorySegment
    @State private var relations: [StorySegmentRelation]
    let save: (StorySegment, [StorySegmentRelation]) -> Void

    init(project: StoryProject, segment: StorySegment, save: @escaping (StorySegment, [StorySegmentRelation]) -> Void) {
        self.project = project; _segment = State(initialValue: segment)
        _relations = State(initialValue: project.relations(for: segment.id)); self.save = save
    }

    private var valid: Bool {
        var draft = project
        guard let index = draft.segments.firstIndex(where: { $0.id == segment.id }) else { return false }
        draft.segments[index] = segment
        draft.relations.removeAll { $0.segmentID == segment.id }; draft.relations += relations
        return !segment.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && (try? draft.validate()) != nil
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(appModel.localized("编辑 15 秒分段", english: "Edit 15-second Segment")).font(.title2.bold())
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    TextField(appModel.localized("分段标题", english: "Segment Title"), text: $segment.title)
                    Text(appModel.localized("本段剧情", english: "Segment Story")).font(.headline)
                    TextEditor(text: $segment.synopsis).frame(height: 70)
                    resourcePicker
                    relationsEditor
                    if segment.detail != nil { detailEditor }
                    else {
                        Button(appModel.localized("手动编写镜头计划", english: "Write Shot Plan Manually")) {
                            segment.detail = .init(firstFramePrompt: "", shots: [
                                .init(start: 0, end: 5, prompt: ""), .init(start: 5, end: 10, prompt: ""), .init(start: 10, end: 15, prompt: ""),
                            ], continuityIn: "", continuityOut: "", audio: "", constraints: "")
                        }
                    }
                }.padding(8)
            }
            Text(appModel.localized("关联关系是生成上下文的唯一来源。修改后需重新确认首帧；已生成视频不被覆盖。", english: "Relations are the source of truth for generation context. Confirm the first frame again after editing; generated videos are retained."))
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button(appModel.localized("取消", english: "Cancel")) { dismiss() }.keyboardShortcut(.cancelAction)
                Button(appModel.localized("保存分段", english: "Save Segment")) { save(segment, relations); dismiss() }
                    .buttonStyle(.borderedProminent).tint(.purple).disabled(!valid)
            }
        }.padding(22).frame(width: 680, height: 760)
    }

    private var resourcePicker: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(appModel.localized("分段外键（人物 / 场景 / 道具，合计最多 8 个）", english: "Segment Links (Characters / Scenes / Props, 8 Total)")).font(.headline)
            ForEach(project.resources) { resource in
                Toggle(resource.name + " · " + resource.kind.rawValue, isOn: Binding(
                    get: { contains(resource) },
                    set: { on in set(resource, enabled: on) }
                )).toggleStyle(.checkbox)
            }
        }
    }
    private func contains(_ resource: StoryResource) -> Bool {
        switch resource.kind {
        case .character: segment.characterIDs.contains(resource.id)
        case .scene: segment.sceneIDs.contains(resource.id)
        case .prop: segment.propIDs.contains(resource.id)
        }
    }
    private func set(_ resource: StoryResource, enabled: Bool) {
        switch resource.kind {
        case .character:
            if enabled { if !segment.characterIDs.contains(resource.id) { segment.characterIDs.append(resource.id) } }
            else { segment.characterIDs.removeAll { $0 == resource.id }; relations.removeAll { $0.characterID == resource.id } }
        case .scene:
            if enabled { if !segment.sceneIDs.contains(resource.id) { segment.sceneIDs.append(resource.id) } }
            else { segment.sceneIDs.removeAll { $0 == resource.id }; relations.removeAll { $0.sceneID == resource.id } }
        case .prop:
            if enabled { if !segment.propIDs.contains(resource.id) { segment.propIDs.append(resource.id) } }
            else { segment.propIDs.removeAll { $0 == resource.id } }
        }
    }

    private var characters: [StoryCharacter] { project.characters.filter { segment.characterIDs.contains($0.id) } }
    private var scenes: [StoryScene] { project.scenes.filter { segment.sceneIDs.contains($0.id) } }
    private var relationsEditor: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(appModel.localized("人物—场景关联表", english: "Character–Scene Relations")).font(.headline)
            Text(appModel.localized("每条记录指定分段、人物、场景、动作、位置和 0–15 秒有效区间。AI 工具与手动编辑写入同一张表。", english: "Each row stores segment, character, scene, action, position and its 0–15 second interval. AI tools and manual edits write the same table."))
                .font(.caption).foregroundStyle(.secondary)
            ForEach(relations.indices, id: \.self) { index in
                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Picker(appModel.localized("人物", english: "Character"), selection: relationBinding(index, \.characterID)) {
                            ForEach(characters) { Text($0.name).tag($0.id) }
                        }
                        Picker(appModel.localized("场景", english: "Scene"), selection: relationBinding(index, \.sceneID)) {
                            ForEach(scenes) { Text($0.name).tag($0.id) }
                        }
                        Button(appModel.localized("移除关系", english: "Remove Relation")) { relations.remove(at: index) }
                    }
                    TextField(appModel.localized("动作与互动", english: "Action & Interaction"), text: relationBinding(index, \.action), axis: .vertical)
                    TextField(appModel.localized("空间位置", english: "Spatial Position"), text: relationBinding(index, \.position), axis: .vertical)
                    HStack {
                        Stepper("\(relations[index].startSecond)s", value: $relations[index].startSecond, in: 0...14)
                        Text("→")
                        Stepper("\(relations[index].endSecond)s", value: $relations[index].endSecond, in: 1...15)
                    }.font(.caption.monospacedDigit())
                }.padding(8).background(Color.purple.opacity(0.04), in: RoundedRectangle(cornerRadius: 8))
            }
            Button(appModel.localized("添加人物—场景关系", english: "Add Character–Scene Relation")) {
                guard let character = characters.first, let scene = scenes.first else { return }
                relations.append(.init(segmentID: segment.id, characterID: character.id, sceneID: scene.id, action: "", position: ""))
            }.disabled(characters.isEmpty || scenes.isEmpty || relations.count >= 8)
        }
    }

    @ViewBuilder private var detailEditor: some View {
        if let detail = segment.detail {
            Text(appModel.localized("首帧提示词", english: "First Frame Prompt")).font(.headline)
            TextEditor(text: detailBinding(\.firstFramePrompt)).frame(height: 65)
            ForEach(Array(detail.shots.enumerated()), id: \.offset) { index, shot in
                Text("\(shot.start)–\(shot.end)s").font(.headline).foregroundStyle(.purple)
                TextEditor(text: Binding(get: { segment.detail?.shots[index].prompt ?? "" }, set: { segment.detail?.shots[index].prompt = $0 })).frame(height: 65)
            }
            TextField(appModel.localized("入镜衔接", english: "Entry Continuity"), text: detailBinding(\.continuityIn), axis: .vertical)
            TextField(appModel.localized("出镜衔接", english: "Exit Continuity"), text: detailBinding(\.continuityOut), axis: .vertical)
            TextField(appModel.localized("声音", english: "Audio"), text: detailBinding(\.audio), axis: .vertical)
            TextField(appModel.localized("一致性约束", english: "Consistency Constraints"), text: detailBinding(\.constraints), axis: .vertical)
        }
    }
    private func relationBinding(_ index: Int, _ path: WritableKeyPath<StorySegmentRelation, String>) -> Binding<String> {
        Binding(get: { relations.indices.contains(index) ? relations[index][keyPath: path] : "" },
                set: { value in if relations.indices.contains(index) { relations[index][keyPath: path] = value } })
    }
    private func detailBinding(_ path: WritableKeyPath<StorySegmentDetail, String>) -> Binding<String> {
        Binding(get: { segment.detail?[keyPath: path] ?? "" }, set: { segment.detail?[keyPath: path] = $0 })
    }
}
