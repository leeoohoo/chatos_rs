import AppKit
import AVKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

struct MediaStudioView: View {
    @EnvironmentObject private var appModel: AppModel
    @ObservedObject var viewModel: MediaStudioViewModel
    @State private var showsGeneratedImagePicker = false
    @State private var showsGeneratedReferencePicker = false
    @State private var imagePreview: MediaStudioImagePreviewRequest?
    @State private var videoPreview: MediaStudioViewModel.VideoHistoryItem?

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            content
        }
        .background(studioBackground)
        .task { viewModel.loadIfNeeded() }
        .onChange(of: viewModel.section) { _, section in
            if section == .story { viewModel.stories.backToList() }
        }
        .sheet(item: $imagePreview) { request in
            MediaStudioImagePreview(request: request) { asset in
                viewModel.useGeneratedImageForVideo(asset)
            }
            .environmentObject(appModel)
        }
        .sheet(item: $videoPreview) { item in
            MediaStudioVideoPreview(item: item)
                .environmentObject(appModel)
        }
        .sheet(isPresented: $showsGeneratedImagePicker) {
            MediaStudioGeneratedImagePicker(viewModel: viewModel)
                .environmentObject(appModel)
        }
        .sheet(isPresented: $showsGeneratedReferencePicker) {
            MediaStudioGeneratedReferencePicker(viewModel: viewModel)
                .environmentObject(appModel)
        }
        .onChange(of: appModel.authentication.phase) { _, _ in
            imagePreview = nil
            videoPreview = nil
            showsGeneratedImagePicker = false
            showsGeneratedReferencePicker = false
        }
    }

    private var studioBackground: some View {
        ZStack {
            Color(nsColor: .windowBackgroundColor)
            LinearGradient(
                colors: [
                    Color.purple.opacity(0.055),
                    Color.clear,
                    Color.blue.opacity(0.025),
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        }
    }

    private var header: some View {
        HStack(spacing: 16) {
            ZStack {
                RoundedRectangle(cornerRadius: 11)
                    .fill(
                        LinearGradient(
                            colors: [.purple, .pink.opacity(0.82)],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        )
                    )
                Image(systemName: "wand.and.stars")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(.white)
            }
            .frame(width: 38, height: 38)
            .shadow(color: .purple.opacity(0.22), radius: 8, y: 3)

            VStack(alignment: .leading, spacing: 2) {
                Text(appModel.localized("AI 创作", english: "AI Creation"))
                    .font(.system(size: 16, weight: .semibold))
                Text(appModel.localized(
                    "独立于聊天的图片与视频工作台",
                    english: "A dedicated image and video workspace"
                ))
                    .font(.system(size: 11.5))
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 24)

            Picker("", selection: $viewModel.section) {
                Label(appModel.localized("图片", english: "Image"), systemImage: "photo")
                    .tag(MediaStudioViewModel.Section.image)
                Label(appModel.localized("视频", english: "Video"), systemImage: "video")
                    .tag(MediaStudioViewModel.Section.video)
                Label(appModel.localized("剧情模式", english: "Story"), systemImage: "film.stack")
                    .tag(MediaStudioViewModel.Section.story)
                Label(appModel.localized("记录", english: "History"), systemImage: "clock")
                    .tag(MediaStudioViewModel.Section.history)
            }
            .labelsHidden()
            .pickerStyle(.segmented)
            .frame(width: 400)
        }
        .padding(.horizontal, 20)
        .frame(height: 68)
        .background(.bar)
    }

    @ViewBuilder
    private var content: some View {
        switch viewModel.section {
        case .image:
            imageWorkspace
        case .video:
            videoWorkspace
        case .story:
            StoryStudioView(viewModel: viewModel.stories, mediaStudio: viewModel)
        case .history:
            historyWorkspace
        }
    }

    private var imageWorkspace: some View {
        HStack(alignment: .top, spacing: 18) {
            resultSurface
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            settingsSurface
                .frame(width: 356)
        }
        .padding(20)
    }

    private var resultSurface: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(appModel.localized("创作画布", english: "Creation Canvas"))
                        .font(.system(size: 15, weight: .semibold))
                    Text(canvasSubtitle)
                        .font(.system(size: 11.5))
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if viewModel.isGenerating {
                    HStack(spacing: 7) {
                        ProgressView().controlSize(.small)
                        Text(appModel.localized("正在生成", english: "Generating"))
                    }
                    .font(.system(size: 11.5, weight: .medium))
                    .foregroundStyle(.purple)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(Color.purple.opacity(0.09), in: Capsule())
                } else if !viewModel.history.isEmpty {
                    Label(
                        appModel.localized("已完成", english: "Complete"),
                        systemImage: "checkmark.circle.fill"
                    )
                    .font(.system(size: 11.5, weight: .medium))
                    .foregroundStyle(.green)
                }
            }
            .padding(.horizontal, 18)
            .frame(height: 62)

            Divider()

            ScrollView {
                Group {
                    if let latest = viewModel.history.first {
                        latestResult(latest)
                    } else {
                        emptyCanvas
                    }
                }
                .padding(18)
            }
        }
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
        .overlay {
            RoundedRectangle(cornerRadius: 16)
                .stroke(Color.primary.opacity(0.08))
        }
        .shadow(color: .black.opacity(0.045), radius: 12, y: 5)
    }

    private var canvasSubtitle: String {
        if let latest = viewModel.history.first {
            return "\(latest.modelName) · \(latest.images.count) "
                + appModel.localized("张图片", english: "images")
        }
        return appModel.localized(
            "结果会显示在这里，不会进入聊天记录",
            english: "Results appear here, outside your chat history"
        )
    }

    private func latestResult(_ latest: MediaStudioViewModel.HistoryItem) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 280), spacing: 14)],
                spacing: 14
            ) {
                ForEach(latest.images) { asset in
                    VStack(spacing: 8) {
                        previewThumbnail(asset, images: latest.images)
                        Button {
                            viewModel.useGeneratedImageForVideo(asset)
                        } label: {
                            Label(appModel.localized("用作视频首帧", english: "Use as Video First Frame"), systemImage: "video.badge.plus")
                        }
                        .buttonStyle(.borderless)
                        .font(.system(size: 12, weight: .medium))
                    }
                }
            }

            VStack(alignment: .leading, spacing: 6) {
                Text(appModel.localized("提示词", english: "Prompt"))
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.secondary)
                Text(latest.prompt)
                    .font(.system(size: 12.5))
                    .textSelection(.enabled)
            }
            .padding(13)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 11))
        }
    }

    private var emptyCanvas: some View {
        VStack(spacing: 22) {
            Spacer(minLength: 38)

            ZStack {
                Circle()
                    .fill(
                        LinearGradient(
                            colors: [.purple.opacity(0.16), .pink.opacity(0.09)],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        )
                    )
                Image(systemName: "photo.on.rectangle.angled")
                    .font(.system(size: 34, weight: .light))
                    .foregroundStyle(
                        LinearGradient(
                            colors: [.purple, .pink],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        )
                    )
            }
            .frame(width: 84, height: 84)

            VStack(spacing: 7) {
                Text(appModel.localized("描述你想看到的画面", english: "Describe what you want to see"))
                    .font(.system(size: 20, weight: .semibold, design: .rounded))
                Text(emptyCanvasDescription)
                    .font(.system(size: 12.5))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 480)
            }

            HStack(spacing: 8) {
                suggestionChip(
                    appModel.localized("电影感人像", english: "Cinematic portrait"),
                    prompt: appModel.localized(
                        "电影感人像摄影，柔和侧光，细腻肤质，浅景深，真实自然",
                        english: "Cinematic portrait photography, soft side lighting, natural skin texture, shallow depth of field"
                    )
                )
                suggestionChip(
                    appModel.localized("产品海报", english: "Product poster"),
                    prompt: appModel.localized(
                        "极简产品广告海报，干净背景，工作室灯光，高级材质与留白",
                        english: "Minimal product advertising poster, clean background, studio lighting, premium materials and generous negative space"
                    )
                )
                suggestionChip(
                    appModel.localized("概念场景", english: "Concept scene"),
                    prompt: appModel.localized(
                        "宏大的未来城市概念场景，黄昏薄雾，电影级构图，丰富空间层次",
                        english: "Epic futuristic city concept scene at dusk, atmospheric haze, cinematic composition, rich depth"
                    )
                )
            }

            Spacer(minLength: 38)
        }
        .frame(maxWidth: .infinity, minHeight: 480)
        .background(
            LinearGradient(
                colors: [Color.clear, Color.purple.opacity(0.025)],
                startPoint: .top,
                endPoint: .bottom
            ),
            in: RoundedRectangle(cornerRadius: 13)
        )
    }

    private func suggestionChip(_ title: String, prompt: String) -> some View {
        Button(title) {
            viewModel.prompt = prompt
        }
        .buttonStyle(.plain)
        .font(.system(size: 11.5, weight: .medium))
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .background(Color.primary.opacity(0.055), in: Capsule())
        .overlay { Capsule().stroke(Color.primary.opacity(0.07)) }
    }

    private var settingsSurface: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack {
                    Text(appModel.localized("生成设置", english: "Generation Settings"))
                        .font(.system(size: 15, weight: .semibold))
                    Spacer()
                    Button {
                        viewModel.reloadModels()
                    } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                    .buttonStyle(.borderless)
                    .disabled(viewModel.isLoadingModels)
                    .help(appModel.localized("刷新模型", english: "Refresh models"))
                }

                modelSelector

                inputImageSelector

                VStack(alignment: .leading, spacing: 8) {
                    fieldLabel(appModel.localized("提示词", english: "Prompt"))
                    TextEditor(text: $viewModel.prompt)
                        .font(.system(size: 13))
                        .scrollContentBackground(.hidden)
                        .padding(10)
                        .frame(minHeight: 176)
                        .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 11))
                        .overlay {
                            RoundedRectangle(cornerRadius: 11)
                                .stroke(Color.primary.opacity(0.11))
                        }
                }

                HStack(alignment: .top, spacing: 12) {
                    VStack(alignment: .leading, spacing: 8) {
                        fieldLabel(appModel.localized("尺寸", english: "Size"))
                        Picker("", selection: $viewModel.size) {
                            Text(appModel.localized("自动", english: "Auto")).tag("auto")
                            Text("1024 × 1024").tag("1024x1024")
                            Text("1536 × 1024").tag("1536x1024")
                            Text("1024 × 1536").tag("1024x1536")
                        }
                        .labelsHidden()
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)

                    VStack(alignment: .leading, spacing: 8) {
                        fieldLabel(appModel.localized("数量", english: "Count"))
                        Stepper(value: $viewModel.count, in: 1...4) {
                            Text("\(viewModel.count)")
                                .monospacedDigit()
                                .frame(minWidth: 16)
                        }
                    }
                    .frame(width: 92, alignment: .leading)
                }

                if let errorMessage = viewModel.errorMessage {
                    HStack(alignment: .top, spacing: 9) {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(.orange)
                        Text(errorMessage)
                            .font(.system(size: 11.5))
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .padding(11)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: 10))
                }

                Button {
                    viewModel.generate()
                } label: {
                    HStack(spacing: 9) {
                        if viewModel.isGenerating {
                            ProgressView().controlSize(.small)
                        } else {
                            Image(systemName: "sparkles")
                        }
                        Text(generateButtonTitle)
                            .fontWeight(.semibold)
                    }
                    .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .tint(.purple)
                .controlSize(.large)
                .disabled(!viewModel.canGenerate)
            }
            .padding(18)
        }
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
        .overlay {
            RoundedRectangle(cornerRadius: 16)
                .stroke(Color.primary.opacity(0.08))
        }
        .shadow(color: .black.opacity(0.045), radius: 12, y: 5)
    }

    @ViewBuilder
    private var modelSelector: some View {
        VStack(alignment: .leading, spacing: 8) {
            fieldLabel(appModel.localized("模型", english: "Model"))

            if viewModel.isLoadingModels && viewModel.models.isEmpty {
                HStack(spacing: 9) {
                    ProgressView().controlSize(.small)
                    Text(appModel.localized("正在读取模型…", english: "Loading models…"))
                        .foregroundStyle(.secondary)
                    Spacer()
                }
                .padding(.horizontal, 12)
                .frame(height: 48)
                .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 10))
            } else if viewModel.models.isEmpty {
                VStack(alignment: .leading, spacing: 5) {
                    Text(appModel.localized("没有可用模型", english: "No usable models"))
                        .font(.system(size: 12.5, weight: .semibold))
                    Text(appModel.localized(
                        "请先配置一个全局启用且具有 API 密钥的模型。",
                        english: "Configure a globally enabled model with an API key first."
                    ))
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 10))
            } else {
                Menu {
                    ForEach(viewModel.models) { item in
                        Button {
                            viewModel.selectedModelID = item.id
                        } label: {
                            if item.id == viewModel.selectedModelID {
                                Label("\(item.name) · \(item.modelName)", systemImage: "checkmark")
                            } else {
                                Text("\(item.name) · \(item.modelName)")
                            }
                        }
                    }
                } label: {
                    HStack(spacing: 11) {
                        ZStack {
                            RoundedRectangle(cornerRadius: 8)
                                .fill(Color.purple.opacity(0.11))
                            Image(systemName: "sparkles")
                                .font(.system(size: 13, weight: .medium))
                                .foregroundStyle(.purple)
                        }
                        .frame(width: 34, height: 34)

                        if let selected = viewModel.selectedModel {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(selected.name)
                                    .font(.system(size: 12.5, weight: .semibold))
                                    .lineLimit(1)
                                Text("\(selected.provider) · \(selected.modelName)")
                                    .font(.system(size: 10.5, design: .monospaced))
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                        }
                        Spacer()
                        Image(systemName: "chevron.up.chevron.down")
                            .font(.system(size: 10, weight: .semibold))
                            .foregroundStyle(.tertiary)
                    }
                    .padding(.horizontal, 9)
                    .frame(height: 52)
                    .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 10))
                    .overlay {
                        RoundedRectangle(cornerRadius: 10)
                            .stroke(Color.primary.opacity(0.11))
                    }
                }
                .buttonStyle(.plain)

                if let selected = viewModel.selectedModel, !selected.taskEnabled {
                    Label(
                        appModel.localized(
                            "未用于任务，但仍可用于独立创作",
                            english: "Not used for tasks, but available for creation"
                        ),
                        systemImage: "info.circle"
                    )
                    .font(.system(size: 10.5))
                    .foregroundStyle(.secondary)
                }
            }
        }
    }

    private var inputImageSelector: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                fieldLabel(appModel.localized("参考图（可多选）", english: "Reference Images (Multiple)"))
                Spacer()
                if !viewModel.inputImages.isEmpty {
                    Text("\(viewModel.inputImages.count) / 8")
                        .font(.system(size: 9.5, weight: .semibold))
                        .foregroundStyle(.purple)
                        .padding(.horizontal, 7)
                        .padding(.vertical, 3)
                        .background(Color.purple.opacity(0.09), in: Capsule())
                }
            }

            Button { showsGeneratedReferencePicker = true } label: {
                Label(appModel.localized("从生成记录选择多张", english: "Choose Multiple from Creations"),
                      systemImage: "photo.stack")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .tint(.purple)
            .disabled(viewModel.inputImages.count >= 8 || viewModel.isGenerating || viewModel.isLoadingInputImages)

            if !viewModel.inputImages.isEmpty {
                VStack(alignment: .leading, spacing: 9) {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 92), spacing: 8)], spacing: 8) {
                        ForEach(Array(viewModel.inputImages.enumerated()), id: \.offset) { index, input in
                            inputReferenceCard(input, index: index)
                        }
                    }
                    HStack {
                        Button { chooseInputImages() } label: {
                            Label(appModel.localized("继续添加", english: "Add More"), systemImage: "plus")
                        }.disabled(viewModel.inputImages.count >= 8 || viewModel.isGenerating || viewModel.isLoadingInputImages)
                        Spacer()
                        Button(appModel.localized("全部清除", english: "Clear All"), role: .destructive) {
                            viewModel.removeInputImage()
                        }.disabled(viewModel.isGenerating || viewModel.isLoadingInputImages)
                    }.buttonStyle(.borderless).font(.system(size: 10.5, weight: .medium))
                }
                .padding(9)
                .background(Color.purple.opacity(0.055), in: RoundedRectangle(cornerRadius: 11))
                .overlay {
                    RoundedRectangle(cornerRadius: 11)
                        .stroke(Color.purple.opacity(0.16))
                }
            } else {
                Button {
                    chooseInputImages()
                } label: {
                    VStack(spacing: 7) {
                        Image(systemName: "photo.badge.plus")
                            .font(.system(size: 19, weight: .light))
                            .foregroundStyle(.purple)
                        Text(appModel.localized(
                            "添加一张或多张参考图",
                            english: "Add One or More Reference Images"
                        ))
                            .font(.system(size: 11.5, weight: .medium))
                        Text(appModel.localized("最多 8 张；每张最大 20 MB", english: "Up to 8 images; 20 MB each"))
                            .font(.system(size: 10))
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 14)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .background(Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 11))
                .overlay {
                    RoundedRectangle(cornerRadius: 11)
                        .stroke(Color.primary.opacity(0.11), style: StrokeStyle(lineWidth: 1, dash: [5, 4]))
                }
            }
            if viewModel.isLoadingInputImages {
                HStack(spacing: 7) {
                    ProgressView().controlSize(.small)
                    Text(appModel.localized("正在读取参考图…", english: "Loading References…"))
                }.font(.caption).foregroundStyle(.secondary)
            }
        }
        .dropDestination(for: URL.self) { urls, _ in
            guard !urls.isEmpty else { return false }
            viewModel.addInputImages(from: urls)
            return true
        }
    }

    private func chooseInputImages() {
        let panel = NSOpenPanel()
        panel.title = appModel.localized("选择参考图", english: "Choose Reference Images")
        panel.message = appModel.localized(
            "可一次选择多张图片，最多添加 8 张",
            english: "Select multiple images at once, up to 8 in total"
        )
        panel.prompt = appModel.localized("添加", english: "Add")
        panel.allowedContentTypes = [.image]
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = true
        guard panel.runModal() == .OK else { return }
        viewModel.addInputImages(from: panel.urls)
    }

    private func inputReferenceCard(_ input: ImageGenerationInputImage, index: Int) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            ZStack(alignment: .topTrailing) {
                Group {
                    if let data = Data(base64Encoded: input.base64Data), let image = NSImage(data: data) {
                        Image(nsImage: image).resizable().scaledToFill()
                    } else { Color.primary.opacity(0.04) }
                }
                .frame(height: 68).clipped().clipShape(RoundedRectangle(cornerRadius: 7))
                Button { viewModel.removeInputImage(at: index) } label: {
                    Image(systemName: "xmark.circle.fill").symbolRenderingMode(.palette)
                        .foregroundStyle(.white, Color.black.opacity(0.58))
                }.buttonStyle(.plain).padding(4).disabled(viewModel.isGenerating)
            }
            Text("#\(index + 1) · \(input.name)").font(.system(size: 9.5, weight: .medium)).lineLimit(1)
        }
    }

    private var emptyCanvasDescription: String {
        if viewModel.inputImage != nil {
            return appModel.localized(
                "参考图已经添加。请在右侧描述希望保留、修改或重新设计的内容。",
                english: "A reference image is ready. Describe what to preserve, change, or redesign."
            )
        }
        return appModel.localized(
            "从一个清晰的主体、场景或风格开始，也可以添加参考图进行图生图。",
            english: "Start with a subject, scene, or style, or add a reference image for image-to-image creation."
        )
    }

    private var generateButtonTitle: String {
        if viewModel.isGenerating {
            return appModel.localized("正在生成…", english: "Generating…")
        }
        if viewModel.inputImage != nil {
            return appModel.localized("基于参考图生成", english: "Generate from Reference")
        }
        return appModel.localized("生成图片", english: "Generate Image")
    }

    private var videoWorkspace: some View {
        HStack(alignment: .top, spacing: 18) {
            videoResultSurface
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            videoSettingsSurface
                .frame(width: 356)
        }
        .padding(20)
    }

    private var videoResultSurface: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text(appModel.localized("视频画布", english: "Video Canvas"))
                        .font(.system(size: 15, weight: .semibold))
                    Text(videoCanvasSubtitle)
                        .font(.system(size: 11.5))
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if viewModel.isGeneratingVideo {
                    Button(appModel.localized("停止等待", english: "Stop Waiting")) {
                        viewModel.cancelVideoGeneration()
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                }
            }
            .padding(.horizontal, 18)
            .frame(height: 62)

            Divider()

            Group {
                switch viewModel.videoCanvasState {
                case let .progress(progress):
                    videoProgressSurface(progress)
                case .failed:
                    videoProgressSurface(.init(status: "failed"))
                case let .result(latest):
                    VStack(alignment: .leading, spacing: 14) {
                        LocalVideoPlayer(url: latest.fileURL)
                            .frame(maxWidth: .infinity, minHeight: 420)
                            .clipShape(RoundedRectangle(cornerRadius: 13))
                        Text(latest.prompt)
                            .font(.system(size: 12.5))
                            .textSelection(.enabled)
                            .padding(13)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 11))
                    }
                    .padding(18)
                case .empty:
                    VStack(spacing: 18) {
                        Spacer()
                        Image(systemName: "video.badge.plus")
                            .font(.system(size: 48, weight: .light))
                            .foregroundStyle(.purple)
                        Text(appModel.localized("描述一段有运动和镜头感的画面", english: "Describe a moving cinematic scene"))
                            .font(.system(size: 20, weight: .semibold, design: .rounded))
                        Text(appModel.localized(
                            "视频任务会在模型提供方异步生成，完成后自动下载到本机。",
                            english: "Video jobs run asynchronously at the model provider and download automatically when complete."
                        ))
                            .font(.system(size: 12.5))
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                        Spacer()
                    }
                    .frame(maxWidth: .infinity, minHeight: 520)
                }
            }
        }
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
        .overlay { RoundedRectangle(cornerRadius: 16).stroke(Color.primary.opacity(0.08)) }
        .shadow(color: .black.opacity(0.045), radius: 12, y: 5)
    }

    private var videoCanvasSubtitle: String {
        switch viewModel.videoCanvasState {
        case let .progress(progress): return videoProgressText(progress)
        case .failed: return appModel.localized("生成失败", english: "Failed")
        case let .result(latest):
            return "\(latest.modelName) · " + appModel.localized("已保存到本机", english: "Saved locally")
        case .empty:
            return appModel.localized("视频结果不会进入聊天记录", english: "Video results stay outside chat history")
        }
    }

    private func videoProgressSurface(_ progress: VideoGenerationProgress) -> some View {
        VStack(spacing: 18) {
            Spacer()
            Image(systemName: progress.status == "failed" ? "exclamationmark.triangle" : "video.badge.waveform")
                .font(.system(size: 44, weight: .light))
                .foregroundStyle(progress.status == "failed" ? Color.orange : Color.purple)
            Text(videoProgressText(progress))
                .font(.system(size: 20, weight: .semibold, design: .rounded))
            if viewModel.isGeneratingVideo {
                if let percent = progress.percent {
                    ProgressView(value: min(100, max(0, percent)), total: 100)
                        .frame(maxWidth: 360)
                } else {
                    ProgressView().controlSize(.large)
                }
                Text(appModel.localized("当前任务完成后会在这里显示新视频。", english: "The new video will appear here when this task completes."))
                    .font(.system(size: 12.5))
                    .foregroundStyle(.secondary)
            }
            if !viewModel.videoHistory.isEmpty {
                Text(appModel.localized("之前生成的视频仍保留在创作记录中。", english: "Previous videos are still available in Creation History."))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func videoProgressText(_ progress: VideoGenerationProgress) -> String {
        let status: String
        switch progress.status {
        case "submitting": status = appModel.localized("提交中", english: "Submitting")
        case "queued": status = appModel.localized("排队中", english: "Queued")
        case "in_progress": status = appModel.localized("生成中", english: "Generating")
        case "downloading": status = appModel.localized("下载中", english: "Downloading")
        case "saving": status = appModel.localized("保存中", english: "Saving")
        case "failed": status = appModel.localized("生成失败", english: "Failed")
        case "completed": status = appModel.localized("已完成", english: "Complete")
        default: status = progress.status
        }
        if let percent = progress.percent {
            return "\(status) · \(Int(percent.rounded()))%"
        }
        return status
    }

    private var videoSettingsSurface: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack {
                    Text(appModel.localized("视频设置", english: "Video Settings"))
                        .font(.system(size: 15, weight: .semibold))
                    Spacer()
                    Button { viewModel.reloadModels() } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                    .buttonStyle(.borderless)
                    .disabled(viewModel.isLoadingModels)
                    .help(appModel.localized("刷新模型", english: "Refresh models"))
                }

                videoModelSelector
                videoInputImageSelector

                VStack(alignment: .leading, spacing: 8) {
                    fieldLabel(appModel.localized("提示词", english: "Prompt"))
                    TextEditor(text: $viewModel.videoPrompt)
                        .font(.system(size: 13))
                        .scrollContentBackground(.hidden)
                        .padding(10)
                        .frame(minHeight: 150)
                        .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 11))
                        .overlay { RoundedRectangle(cornerRadius: 11).stroke(Color.primary.opacity(0.11)) }
                }

                HStack(alignment: .top, spacing: 12) {
                    VStack(alignment: .leading, spacing: 8) {
                        fieldLabel(viewModel.videoProfile.isMiniMax
                                   ? appModel.localized("分辨率", english: "Resolution")
                                   : appModel.localized("画幅", english: "Frame"))
                        Picker("", selection: $viewModel.videoSize) {
                            ForEach(viewModel.videoProfile.sizes, id: \.self) { size in
                                Text(size.replacingOccurrences(of: "x", with: " × ")).tag(size)
                            }
                        }
                        .labelsHidden()
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)

                    VStack(alignment: .leading, spacing: 8) {
                        fieldLabel(appModel.localized("时长", english: "Duration"))
                        Picker("", selection: $viewModel.videoSeconds) {
                            ForEach(viewModel.videoProfile.durations, id: \.self) { seconds in
                                Text("\(seconds)s").tag(seconds)
                            }
                        }
                        .labelsHidden()
                    }
                    .frame(width: 90, alignment: .leading)
                }

                if viewModel.videoProfile.isMiniMax {
                    VStack(alignment: .leading, spacing: 8) {
                        fieldLabel(appModel.localized("画面比例", english: "Aspect Ratio"))
                        if viewModel.videoInputImage != nil {
                            Text(appModel.localized("跟随参考图比例", english: "Matches the reference image"))
                                .font(.system(size: 11.5))
                                .foregroundStyle(.secondary)
                        } else {
                            Picker("", selection: $viewModel.videoRatio) {
                                ForEach(VideoGenerationProfile.miniMaxRatios, id: \.self) { ratio in
                                    Text(ratio).tag(ratio)
                                }
                            }
                            .labelsHidden()
                        }
                    }
                }

                if let errorMessage = viewModel.errorMessage {
                    Label(errorMessage, systemImage: "exclamationmark.triangle.fill")
                        .font(.system(size: 11.5))
                        .foregroundStyle(.orange)
                        .padding(11)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Color.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: 10))
                }

                Button { viewModel.generateVideo() } label: {
                    HStack(spacing: 9) {
                        if viewModel.isGeneratingVideo {
                            ProgressView().controlSize(.small)
                        } else {
                            Image(systemName: "video.badge.plus")
                        }
                        Text(viewModel.videoInputImage == nil
                             ? appModel.localized("生成视频", english: "Generate Video")
                             : appModel.localized("基于参考图生成视频", english: "Generate from Reference"))
                            .fontWeight(.semibold)
                    }
                    .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .tint(.purple)
                .controlSize(.large)
                .disabled(!viewModel.canGenerateVideo)
            }
            .padding(18)
        }
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
        .overlay { RoundedRectangle(cornerRadius: 16).stroke(Color.primary.opacity(0.08)) }
        .shadow(color: .black.opacity(0.045), radius: 12, y: 5)
    }

    @ViewBuilder
    private var videoModelSelector: some View {
        VStack(alignment: .leading, spacing: 8) {
            fieldLabel(appModel.localized("视频模型", english: "Video Model"))
            if viewModel.isLoadingModels && viewModel.videoModels.isEmpty {
                ProgressView().controlSize(.small)
                    .frame(maxWidth: .infinity, minHeight: 48)
            } else if viewModel.videoModels.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Text(appModel.localized("当前没有兼容视频模型", english: "No compatible video models"))
                        .font(.system(size: 12.5, weight: .semibold))
                    Text(appModel.localized(
                        "当前配置中没有视频模型。添加 MiniMax H3、H3 Max 或兼容的视频模型后刷新。",
                        english: "No video models are configured. Add MiniMax H3, H3 Max, or a compatible video model, then refresh."
                    ))
                        .font(.system(size: 10.5))
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 10))
            } else {
                Picker("", selection: $viewModel.selectedVideoModelID) {
                    ForEach(viewModel.videoModels) { model in
                        Text("\(model.name) · \(model.modelName)").tag(Optional(model.id))
                    }
                }
                .labelsHidden()
            }
        }
    }

    private var videoInputImageSelector: some View {
        VStack(alignment: .leading, spacing: 8) {
            fieldLabel(appModel.localized("首帧参考图（可选）", english: "First-frame Reference (Optional)"))
            Button { showsGeneratedImagePicker = true } label: {
                Label(appModel.localized("从创作记录选择", english: "Choose from Creations"), systemImage: "photo.on.rectangle.angled")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .tint(.purple)
            .disabled(viewModel.isLoadingHistory)
            if viewModel.isLoadingVideoInputImage {
                ProgressView(appModel.localized("正在读取参考图…", english: "Loading reference image…"))
                    .font(.caption)
                    .controlSize(.small)
            }
            if let input = viewModel.videoInputImage,
               let data = Data(base64Encoded: input.base64Data),
               let image = NSImage(data: data) {
                HStack(spacing: 10) {
                    Image(nsImage: image)
                        .resizable()
                        .scaledToFill()
                        .frame(width: 72, height: 56)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                    Text(input.name)
                        .font(.system(size: 11.5, weight: .medium))
                        .lineLimit(1)
                    Spacer()
                    Button { chooseVideoInputImage() } label: {
                        Image(systemName: "arrow.triangle.2.circlepath")
                    }
                    .buttonStyle(.borderless)
                    .help(appModel.localized("从本机更换参考图", english: "Replace from Computer"))
                    Button { viewModel.removeVideoInputImage() } label: {
                        Image(systemName: "xmark")
                    }
                    .buttonStyle(.borderless)
                    .help(appModel.localized("移除参考图", english: "Remove Reference"))
                }
                .padding(9)
                .background(Color.purple.opacity(0.055), in: RoundedRectangle(cornerRadius: 11))
            } else {
                Button { chooseVideoInputImage() } label: {
                    Label(
                        appModel.localized("添加首帧参考图", english: "Add first-frame reference"),
                        systemImage: "photo.badge.plus"
                    )
                    .font(.system(size: 11.5, weight: .medium))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 13)
                }
                .buttonStyle(.plain)
                .background(Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 11))
                .overlay {
                    RoundedRectangle(cornerRadius: 11)
                        .stroke(Color.primary.opacity(0.11), style: StrokeStyle(lineWidth: 1, dash: [5, 4]))
                }
            }
        }
        .dropDestination(for: URL.self) { urls, _ in
            guard let url = urls.first else { return false }
            viewModel.selectVideoInputImage(from: url)
            return true
        }
    }

    private func chooseVideoInputImage() {
        let panel = NSOpenPanel()
        panel.title = appModel.localized("选择视频首帧参考图", english: "Choose a Video First-frame Reference")
        panel.prompt = appModel.localized("选择", english: "Choose")
        panel.allowedContentTypes = [.image]
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        viewModel.selectVideoInputImage(from: url)
    }

    private var historyWorkspace: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(appModel.localized("创作记录", english: "Creation History"))
                        .font(.system(size: 24, weight: .semibold, design: .rounded))
                    Text(appModel.localized(
                        "已自动保存到本机，重启后仍可查看",
                        english: "Saved locally and available after restarting the app"
                    ))
                        .foregroundStyle(.secondary)
                }

                if let message = viewModel.historyErrorMessage {
                    Label(message, systemImage: "exclamationmark.triangle")
                        .foregroundStyle(.orange)
                        .textSelection(.enabled)
                }

                if viewModel.isLoadingHistory {
                    ProgressView(appModel.localized("正在加载记录", english: "Loading history"))
                        .frame(maxWidth: .infinity, minHeight: 200)
                } else if viewModel.history.isEmpty && viewModel.videoHistory.isEmpty {
                    ContentUnavailableView(
                        appModel.localized("还没有创作记录", english: "No Creation History"),
                        systemImage: "clock.arrow.circlepath"
                    )
                    .frame(maxWidth: .infinity, minHeight: 420)
                } else {
                    videoHistorySection
                    imageHistorySection
                }
            }
            .padding(24)
        }
    }

    @ViewBuilder
    private var videoHistorySection: some View {
        if !viewModel.videoHistory.isEmpty {
            Text(appModel.localized("视频", english: "Video"))
                .font(.system(size: 15, weight: .semibold))
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 420), spacing: 14)],
                spacing: 14
            ) {
                ForEach(viewModel.videoHistory) { item in
                    HStack(spacing: 14) {
                        ZStack(alignment: .topTrailing) {
                            LocalVideoPlayer(url: item.fileURL)
                                .frame(width: 144, height: 88)
                                .clipShape(RoundedRectangle(cornerRadius: 9))
                            Button { videoPreview = item } label: {
                                Image(systemName: "arrow.up.left.and.arrow.down.right")
                                    .font(.system(size: 11, weight: .semibold))
                                    .padding(7)
                                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 7))
                                    .padding(7)
                            }
                            .buttonStyle(.plain)
                            .help(appModel.localized("放大播放视频", english: "Enlarge Video"))
                            .accessibilityLabel(appModel.localized("放大播放视频", english: "Enlarge Video"))
                        }
                        historyDescription(
                            prompt: item.prompt,
                            modelName: item.modelName,
                            createdAt: item.createdAt
                        )
                        Spacer()
                        Button {
                            NSWorkspace.shared.activateFileViewerSelecting([item.fileURL])
                        } label: {
                            Image(systemName: "folder")
                        }
                        .buttonStyle(.borderless)
                        .help(appModel.localized("在访达中显示", english: "Show in Finder"))
                    }
                    .padding(12)
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
                    .overlay { RoundedRectangle(cornerRadius: 14).stroke(Color.primary.opacity(0.07)) }
                }
            }
        }
    }

    @ViewBuilder
    private var imageHistorySection: some View {
        if !viewModel.history.isEmpty {
            Text(appModel.localized("图片", english: "Images"))
                .font(.system(size: 15, weight: .semibold))
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 360), spacing: 14)],
                spacing: 14
            ) {
                ForEach(viewModel.history) { item in
                    HStack(spacing: 14) {
                        if let asset = item.images.first {
                            previewThumbnail(asset, images: item.images, compact: true)
                                .frame(width: 108, height: 82)
                        }
                        historyDescription(
                            prompt: item.prompt,
                            modelName: item.modelName,
                            createdAt: item.createdAt
                        )
                        Spacer()
                        Text("\(item.images.count)")
                            .font(.system(size: 11, weight: .semibold, design: .rounded))
                            .padding(7)
                            .background(.quaternary, in: Capsule())
                        if let url = item.images.first?.url, url.isFileURL {
                            Button {
                                NSWorkspace.shared.activateFileViewerSelecting(item.images.compactMap(\.url))
                            } label: {
                                Image(systemName: "folder")
                            }
                            .buttonStyle(.borderless)
                            .help(appModel.localized("在访达中显示", english: "Show in Finder"))
                        }
                    }
                    .padding(12)
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
                    .overlay { RoundedRectangle(cornerRadius: 14).stroke(Color.primary.opacity(0.07)) }
                }
            }
        }
    }

    private func historyDescription(prompt: String, modelName: String, createdAt: Date) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(prompt)
                .font(.system(size: 12.5, weight: .medium))
                .lineLimit(2)
            Text("\(modelName) · \(createdAt.formatted(date: .abbreviated, time: .shortened))")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private func previewThumbnail(_ asset: GeneratedMediaAsset, images: [GeneratedMediaAsset], compact: Bool = false) -> some View {
        Button {
            imagePreview = .init(images: images, selectedIndex: images.firstIndex(of: asset) ?? 0)
        } label: {
            GeneratedMediaAssetView(asset: asset, compact: compact)
                .overlay(alignment: .topTrailing) {
                    Image(systemName: "arrow.up.left.and.arrow.down.right")
                        .font(.system(size: 11, weight: .semibold))
                        .padding(7)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 7))
                        .padding(8)
                }
        }
        .buttonStyle(.plain)
        .help(appModel.localized("放大查看图片", english: "Enlarge Image"))
        .accessibilityLabel(appModel.localized("放大查看图片", english: "Enlarge Image"))
    }

    private func fieldLabel(_ value: String) -> some View {
        Text(value)
            .font(.system(size: 11.5, weight: .semibold))
            .foregroundStyle(.secondary)
    }
}

struct GeneratedMediaAssetView: View {
    let asset: GeneratedMediaAsset
    var compact = false

    var body: some View {
        Group {
            if let data = asset.base64Data.flatMap({ Data(base64Encoded: $0) }),
               let image = NSImage(data: data) {
                Image(nsImage: image)
                    .resizable()
                    .scaledToFit()
            } else if let url = asset.url {
                if url.isFileURL, let image = NSImage(contentsOf: url) {
                    Image(nsImage: image).resizable().scaledToFit()
                } else {
                    AsyncImage(url: url) { phase in
                        switch phase {
                        case let .success(image): image.resizable().scaledToFit()
                        case .failure: unavailable
                        case .empty: ProgressView()
                        @unknown default: unavailable
                        }
                    }
                }
            } else {
                unavailable
            }
        }
        .frame(maxWidth: .infinity, minHeight: compact ? 82 : 260, maxHeight: compact ? 82 : 620)
        .background(Color.black.opacity(0.035), in: RoundedRectangle(cornerRadius: compact ? 9 : 13))
        .clipShape(RoundedRectangle(cornerRadius: compact ? 9 : 13))
        .overlay {
            RoundedRectangle(cornerRadius: compact ? 9 : 13)
                .stroke(Color.primary.opacity(0.08))
        }
    }

    private var unavailable: some View {
        Image(systemName: "photo.badge.exclamationmark")
            .font(.system(size: compact ? 18 : 32))
            .foregroundStyle(.secondary)
    }
}

struct LocalVideoPlayer: NSViewRepresentable {
    let url: URL

    func makeNSView(context: Context) -> AVPlayerView {
        let view = AVPlayerView()
        view.controlsStyle = .floating
        view.videoGravity = .resizeAspect
        view.player = AVPlayer(url: url)
        return view
    }

    func updateNSView(_ view: AVPlayerView, context: Context) {
        guard let currentURL = (view.player?.currentItem?.asset as? AVURLAsset)?.url,
              currentURL == url else {
            view.player = AVPlayer(url: url)
            return
        }
    }

    static func dismantleNSView(_ view: AVPlayerView, coordinator: ()) {
        view.player?.pause()
        view.player = nil
    }
}

private struct MediaStudioVideoPreview: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    let item: MediaStudioViewModel.VideoHistoryItem

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(appModel.localized("视频预览", english: "Video Preview"))
                        .font(.title2.bold())
                    Text("\(item.modelName) · \(item.createdAt.formatted(date: .abbreviated, time: .shortened))")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button {
                    NSWorkspace.shared.activateFileViewerSelecting([item.fileURL])
                } label: {
                    Label(appModel.localized("在访达中显示", english: "Show in Finder"), systemImage: "folder")
                }
                Button(appModel.localized("关闭", english: "Close")) { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }

            LocalVideoPlayer(url: item.fileURL)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color.black)
                .clipShape(RoundedRectangle(cornerRadius: 12))

            ScrollView {
                Text(item.prompt)
                    .font(.callout)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxHeight: 100)
            .padding(12)
            .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 10))
        }
        .padding(20)
        .frame(minWidth: 900, minHeight: 620)
    }
}
