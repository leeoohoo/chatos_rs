import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

struct StoryImageTarget: Identifiable {
    let id = UUID()
    var assetID: String?
    var segmentID: String?
    var frameRole: StoryFrameRole = .first
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
    @State private var selectedReferenceAssetIDs: Set<String> = []
    private var current: StoryProject { viewModel.projects.first { $0.id == project.id } ?? project }
    private var asset: StoryResource? { current.resources.first { $0.id == target.assetID } }
    private var segment: StorySegment? { current.segments.first { $0.id == target.segmentID } }
    private var images: [StoryImage] {
        if let asset { return asset.images }
        guard let segment else { return [] }
        return segment.frames(for: target.frameRole).images
    }
    private var confirmedID: UUID? {
        if let asset { return asset.confirmedImageID }
        return target.frameRole == .first ? segment?.confirmedFrameID : segment?.confirmedLastFrameID
    }
    private var relatedAssets: [StoryResource] {
        guard let segment else { return [] }
        return segment.resourceIDs.compactMap { current.resource(id: $0) }
    }
    private var selectedReferences: [StoryResource] {
        relatedAssets.filter { selectedReferenceAssetIDs.contains($0.id) && $0.confirmedImage != nil }
    }
    private var previousTailReference: (segment: StorySegment, image: StoryImage)? {
        guard target.frameRole == .first, let segment else { return nil }
        return StoryContinuityContext.previousTail(current, segmentID: segment.id)
    }
    private var generatedRelatedAssets: [StoryResource] { relatedAssets.filter { !$0.images.isEmpty } }
    private var hasUnresolvedImageSubmission: Bool {
        if asset?.imageGenerationAttemptID != nil { return true }
        guard let segment else { return false }
        return target.frameRole == .first
            ? segment.firstFrames.generationAttemptID != nil
            : segment.lastFrames.generationAttemptID != nil
    }
    private var isGeneratingAsset: Bool {
        guard let id = target.assetID else { return false }
        return viewModel.isGeneratingAsset(id, projectID: current.id)
    }

    private var referenceSelection: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text(target.frameRole == .first
                         ? appModel.localized("首帧参考素材", english: "First-frame References")
                         : appModel.localized("尾帧参考素材", english: "Last-frame References")).font(.headline)
                    Text(appModel.localized("可选择一个或多个本段关联素材；顺序将与提示词中的参考编号保持一致。",
                                            english: "Select one or more linked assets. Their order stays aligned with prompt reference numbers."))
                        .font(.caption).foregroundStyle(.secondary)
                    Text(appModel.localized("生成请求还会自动带上本段完整镜头语言，以及所有关联场景、人物、道具的文字画像和关系。",
                                            english: "The request also includes the complete shot plan plus written profiles and relations for every linked scene, character, and prop."))
                        .font(.caption).foregroundStyle(.secondary)
                    if previousTailReference != nil {
                        Text(appModel.localized("上一段已确认尾帧会自动作为额外参考图，不需要手动选择。",
                                                english: "The previous segment's confirmed tail frame is included automatically as an additional continuity reference."))
                            .font(.caption.weight(.semibold)).foregroundStyle(.indigo)
                    }
                }
                Spacer()
                Text(appModel.localized("已选", english: "Selected") + " \(selectedReferences.count)")
                    .font(.caption.weight(.semibold)).foregroundStyle(.indigo)
                    .padding(.horizontal, 9).padding(.vertical, 5)
                    .background(Color.indigo.opacity(0.09), in: Capsule())
            }
            if !generatedRelatedAssets.isEmpty {
                Button {
                    let ids = generatedRelatedAssets.map(\.id)
                    selectedReferenceAssetIDs.formUnion(ids)
                    viewModel.confirmLatestAssetImages(ids)
                } label: {
                    Label(appModel.localized("确认并选择全部已生成素材", english: "Confirm & Select All Generated Assets"),
                          systemImage: "checkmark.circle.fill")
                }
                .buttonStyle(.bordered)
                .tint(.indigo)
                .disabled(viewModel.isBusy || viewModel.hasActiveAssetGenerations || hasUnresolvedImageSubmission)
            }
            if let previousTailReference {
                HStack(spacing: 10) {
                    StoryThumbnail(asset: viewModel.mediaAsset(previousTailReference.image, projectID: current.id))
                        .frame(width: 116, height: 74).clipped()
                        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                    VStack(alignment: .leading, spacing: 4) {
                        Label(appModel.localized("自动衔接上一段尾帧", english: "Previous Tail Included Automatically"),
                              systemImage: "link.circle.fill")
                            .font(.caption.weight(.semibold)).foregroundStyle(.indigo)
                        Text(previousTailReference.segment.title).font(.caption2).foregroundStyle(.secondary)
                        Text(appModel.localized("生成本段首帧时将直接继承这张图的构图与人物状态。",
                                                english: "This image anchors the composition and character state of the new first frame."))
                            .font(.caption2).foregroundStyle(.secondary).lineLimit(2)
                    }
                    Spacer()
                }
                .padding(8)
                .background(Color.indigo.opacity(0.07), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(Color.indigo.opacity(0.18)))
            }
            if relatedAssets.isEmpty {
                Text(appModel.localized("这个分段还没有关联素材，可先编辑分段关系或直接上传首帧。",
                                        english: "This segment has no linked assets. Edit its relations or upload a frame directly."))
                    .font(.callout).foregroundStyle(.secondary)
            } else {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 10) {
                        ForEach(relatedAssets) { resource in referenceCard(resource) }
                    }.padding(.vertical, 2)
                }
            }
        }
        .padding(12)
        .background(Color.indigo.opacity(0.045), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Color.indigo.opacity(0.12)))
    }

    private func referenceCard(_ resource: StoryResource) -> some View {
        let selected = selectedReferenceAssetIDs.contains(resource.id)
        let confirmed = resource.confirmedImage
        let latest = resource.images.last
        let displayed = confirmed ?? latest
        return Button {
            if confirmed != nil {
                if selected { selectedReferenceAssetIDs.remove(resource.id) }
                else { selectedReferenceAssetIDs.insert(resource.id) }
            } else if latest != nil {
                selectedReferenceAssetIDs.insert(resource.id)
                viewModel.confirmLatestAssetImages([resource.id])
            }
        } label: {
            VStack(alignment: .leading, spacing: 7) {
                ZStack(alignment: .topTrailing) {
                    StoryThumbnail(asset: displayed.flatMap { viewModel.mediaAsset($0, projectID: current.id) })
                        .frame(width: 116, height: 74).clipped()
                        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                    Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                        .font(.system(size: 17, weight: .semibold))
                        .foregroundStyle(selected ? Color.indigo : Color.secondary)
                        .background(Color(nsColor: .windowBackgroundColor), in: Circle())
                        .padding(5)
                }
                Text(resource.name).font(.caption.weight(.semibold)).lineLimit(1)
                if confirmed != nil {
                    Text(referenceKind(resource.kind)).font(.caption2).foregroundStyle(.secondary)
                } else if latest != nil {
                    Label(appModel.localized("点击确认并选中", english: "Click to Confirm & Select"),
                          systemImage: "checkmark.circle")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.orange)
                } else {
                    Text(appModel.localized("尚未生成图片", english: "No Generated Image"))
                        .font(.caption2).foregroundStyle(.orange)
                }
            }
            .padding(8).frame(width: 132, alignment: .leading)
            .background(selected ? Color.indigo.opacity(0.1) : Color.primary.opacity(0.025),
                        in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous)
                .stroke(selected ? Color.indigo.opacity(0.6) : Color.primary.opacity(0.06),
                        lineWidth: selected ? 1.5 : 1))
        }
        .buttonStyle(.plain)
        .disabled(displayed == nil || viewModel.isBusy || viewModel.hasActiveAssetGenerations
                  || hasUnresolvedImageSubmission)
    }

    private func referenceKind(_ kind: StoryResource.Kind) -> String {
        switch kind {
        case .character: appModel.localized("角色素材", english: "Character")
        case .scene: appModel.localized("场景素材", english: "Scene")
        case .prop: appModel.localized("道具素材", english: "Prop")
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text(asset?.name ?? (target.frameRole == .first
                     ? appModel.localized("分段首帧", english: "Segment First Frame")
                     : appModel.localized("分段尾帧", english: "Segment Last Frame"))).font(.title2.bold())
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
                }.disabled(viewModel.isBusy || viewModel.hasActiveAssetGenerations || assetPrompt == asset.prompt || assetPrompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                           || assetPrompt.count > 4_000 || current.hasUnresolvedJobs)
            }
            if segment != nil { referenceSelection }
            if let error = viewModel.errorMessage { Text(error).font(.caption).foregroundStyle(.red) }
            if let id = target.assetID, let error = viewModel.assetGenerationError(id, projectID: current.id) {
                Text(error).font(.caption).foregroundStyle(.red)
            }
            if hasUnresolvedImageSubmission {
                VStack(alignment: .leading, spacing: 6) {
                    Text(appModel.localized("上次图片提交结果尚未核实，为避免重复扣费，当前禁止再次提交。", english: "The previous image submission is unresolved, so resubmission is blocked to avoid duplicate charges."))
                        .font(.caption).foregroundStyle(.orange)
                    Button(appModel.localized("核对后允许重试…", english: "Allow Retry after Verification…")) { confirmsImageRetry = true }
                        .disabled(viewModel.isBusy || viewModel.hasActiveAssetGenerations)
                }
            }
            HStack {
                if let id = target.assetID {
                    Button(appModel.localized("生成素材图片", english: "Generate Asset Image")) { viewModel.generateAsset(id) }
                        .buttonStyle(.borderedProminent).tint(.purple)
                        .disabled(viewModel.isBusy || isGeneratingAsset || hasUnresolvedImageSubmission || assetPrompt != asset?.prompt)
                }
                if let segment {
                    Button {
                        viewModel.generateFrame(segment.id, role: target.frameRole,
                                                referenceAssetIDs: selectedReferences.map(\.id))
                    } label: {
                        Label((target.frameRole == .first
                               ? (previousTailReference == nil
                                  ? appModel.localized("用已选素材生成首帧", english: "Generate First Frame from Selected Assets")
                                  : appModel.localized("用已选素材并衔接上一段", english: "Generate and Continue Previous Segment"))
                               : (segment.firstFrame == nil
                                  ? appModel.localized("用已选素材生成尾帧", english: "Generate Last Frame from Selected Assets")
                                  : appModel.localized("用已选素材并衔接首帧", english: "Generate Last Frame from First Frame & Assets")))
                              + " (\(selectedReferences.count))", systemImage: "sparkles.rectangle.stack")
                    }
                    .buttonStyle(.borderedProminent).tint(.indigo)
                    .disabled(viewModel.isBusy || viewModel.hasActiveAssetGenerations || hasUnresolvedImageSubmission
                              || segment.detail == nil || selectedReferences.isEmpty)
                }
                Button(appModel.localized("本机上传", english: "Upload Image")) { showsUpload = true }
                    .disabled(viewModel.isBusy || viewModel.hasActiveAssetGenerations)
                if isGeneratingAsset {
                    ProgressView().controlSize(.small)
                    Text(appModel.localized("正在生成此素材…", english: "Generating this asset…")).font(.caption)
                } else if viewModel.isBusy {
                    ProgressView().controlSize(.small); Text(viewModel.operation).font(.caption)
                }
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
                                    viewModel.confirmImage(image, assetID: target.assetID, segmentID: target.segmentID,
                                                           frameRole: target.frameRole)
                                }.disabled(viewModel.isBusy || viewModel.hasActiveAssetGenerations || confirmedID == image.id)
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
                                Button { viewModel.importImage(image, assetID: target.assetID, segmentID: target.segmentID,
                                                               frameRole: target.frameRole) } label: {
                                    VStack {
                                        StoryThumbnail(asset: image).frame(height: 110).clipped()
                                        Text(record.prompt).font(.caption).lineLimit(2)
                                    }
                                }.buttonStyle(.plain).disabled(viewModel.isBusy || viewModel.hasActiveAssetGenerations)
                            }
                        }
                    }
                }
            }
            Text(appModel.localized("旧版本会保留；确认角色或场景新版本后，相关未生成分段需要重新确认首帧。", english: "Old versions are retained. Changing a shared asset requires affected first frames to be confirmed again."))
                .font(.caption).foregroundStyle(.secondary)
        }.padding(22).frame(width: 760, height: 740)
        .onAppear {
            assetPrompt = asset?.prompt ?? ""
            selectedReferenceAssetIDs = Set(relatedAssets.compactMap { $0.confirmedImage == nil ? nil : $0.id })
        }
        .fileImporter(isPresented: $showsUpload, allowedContentTypes: [.image]) { result in
            switch result {
            case .success(let url): viewModel.uploadImage(url, assetID: target.assetID, segmentID: target.segmentID,
                                                          frameRole: target.frameRole)
            case .failure(let error): mediaStudio.reportInputImageError(error)
            }
        }
        .sheet(item: $preview) { request in
            MediaStudioImagePreview(request: request) { mediaStudio.useGeneratedImageForVideo($0) }.environmentObject(appModel)
        }
        .confirmationDialog(appModel.localized("确认已核对图片任务？", english: "Have You Verified the Image Task?"),
                            isPresented: $confirmsImageRetry, titleVisibility: .visible) {
            Button(appModel.localized("确认未生成，允许重试", english: "Confirmed No Result — Allow Retry")) {
                viewModel.allowImageRetryAfterVerification(assetID: target.assetID, segmentID: target.segmentID,
                                                            frameRole: target.frameRole)
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
            HStack {
                Text(appModel.localized("编辑分段", english: "Edit Segment")).font(.title2.bold())
                Text(segment.kind == .transition
                     ? appModel.localized("转场", english: "Transition")
                     : appModel.localized("剧情", english: "Story"))
                    .font(.caption.bold())
                    .foregroundStyle(segment.kind == .transition ? .purple : .blue)
                    .padding(.horizontal, 9).padding(.vertical, 4)
                    .background((segment.kind == .transition ? Color.purple : Color.blue).opacity(0.1), in: Capsule())
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    TextField(appModel.localized("分段标题", english: "Segment Title"), text: $segment.title)
                    Stepper(appModel.localized("时长：\(segment.seconds) 秒", english: "Duration: \(segment.seconds) seconds"),
                            value: durationBinding,
                            in: segment.kind == .transition ? 2...3 : 2...15)
                    Text(segment.kind == .transition
                         ? appModel.localized("转场段只连接前后画面状态，不额外消耗剧情原文。",
                                              english: "A transition only connects adjacent visual states and consumes no story text.")
                         : appModel.localized("剧情段承载原文内容，时长可按节奏设为2–15秒。",
                                              english: "A story segment carries source content and may last 2–15 seconds."))
                        .font(.caption).foregroundStyle(.secondary)
                    Text(appModel.localized("本段剧情", english: "Segment Story")).font(.headline)
                    TextEditor(text: $segment.synopsis).frame(height: 70)
                    resourcePicker
                    relationsEditor
                    if segment.detail != nil { detailEditor }
                    else {
                        Button(appModel.localized("手动编写镜头计划", english: "Write Shot Plan Manually")) {
                            segment.detail = .init(firstFramePrompt: "", shots: defaultShots,
                                                   continuityIn: "", continuityOut: "", audio: "", constraints: "")
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
            Text(appModel.localized("每条记录指定分段、人物、场景、动作、位置和 0–\(segment.seconds) 秒有效区间。AI 工具与手动编辑写入同一张表。", english: "Each row stores segment, character, scene, action, position and its 0–\(segment.seconds) second interval. AI tools and manual edits write the same table."))
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
                        Stepper("\(relations[index].startSecond)s", value: $relations[index].startSecond,
                                in: 0...max(0, segment.seconds - 1))
                        Text("→")
                        Stepper("\(relations[index].endSecond)s", value: $relations[index].endSecond,
                                in: 1...segment.seconds)
                    }.font(.caption.monospacedDigit())
                }.padding(8).background(Color.purple.opacity(0.04), in: RoundedRectangle(cornerRadius: 8))
            }
            Button(appModel.localized("添加人物—场景关系", english: "Add Character–Scene Relation")) {
                guard let character = characters.first, let scene = scenes.first else { return }
                relations.append(.init(segmentID: segment.id, characterID: character.id, sceneID: scene.id,
                                       action: "", position: "", endSecond: segment.seconds))
            }.disabled(characters.isEmpty || scenes.isEmpty || relations.count >= 8)
        }
    }

    @ViewBuilder private var detailEditor: some View {
        if let detail = segment.detail {
            Text(appModel.localized("首帧提示词", english: "First Frame Prompt")).font(.headline)
            TextEditor(text: detailBinding(\.firstFramePrompt)).frame(height: 65)
            Text(appModel.localized("尾帧提示词", english: "Last Frame Prompt")).font(.headline)
            TextEditor(text: Binding(
                get: { segment.detail?.lastFramePrompt ?? detail.effectiveLastFramePrompt },
                set: { segment.detail?.lastFramePrompt = $0 }
            )).frame(height: 65)
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
    private var durationBinding: Binding<Int> {
        Binding(get: { segment.seconds }, set: { value in
            guard segment.seconds != value else { return }
            segment.seconds = value
            segment.detail = nil
            for index in relations.indices {
                relations[index].endSecond = min(relations[index].endSecond, value)
                relations[index].startSecond = min(relations[index].startSecond, max(0, value - 1))
                if relations[index].endSecond <= relations[index].startSecond {
                    relations[index].endSecond = min(value, relations[index].startSecond + 1)
                }
            }
        })
    }
    private var defaultShots: [StorySegmentDetail.Shot] {
        let count = min(3, segment.seconds)
        return (0..<count).map { index in
            let start = index * segment.seconds / count
            let end = (index + 1) * segment.seconds / count
            return .init(start: start, end: end, prompt: "")
        }
    }
}
