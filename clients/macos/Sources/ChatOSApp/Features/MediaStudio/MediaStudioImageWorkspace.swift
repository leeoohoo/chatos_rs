import AppKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

extension MediaStudioView {
    var imageWorkspace: some View {
        HStack(alignment: .top, spacing: 18) {
            resultSurface
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            settingsSurface
                .frame(width: 356)
        }
        .padding(20)
    }

    var resultSurface: some View {
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

    var canvasSubtitle: String {
        if let latest = viewModel.history.first {
            return "\(latest.modelName) · \(latest.images.count) "
                + appModel.localized("张图片", english: "images")
        }
        return appModel.localized(
            "结果会显示在这里，不会进入聊天记录",
            english: "Results appear here, outside your chat history"
        )
    }

    func latestResult(_ latest: MediaStudioViewModel.HistoryItem) -> some View {
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

    var emptyCanvas: some View {
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

    func suggestionChip(_ title: String, prompt: String) -> some View {
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

    var settingsSurface: some View {
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
    var modelSelector: some View {
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

    var inputImageSelector: some View {
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

    func chooseInputImages() {
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

    func inputReferenceCard(_ input: ImageGenerationInputImage, index: Int) -> some View {
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

    var emptyCanvasDescription: String {
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

    var generateButtonTitle: String {
        if viewModel.isGenerating {
            return appModel.localized("正在生成…", english: "Generating…")
        }
        if viewModel.inputImage != nil {
            return appModel.localized("基于参考图生成", english: "Generate from Reference")
        }
        return appModel.localized("生成图片", english: "Generate Image")
    }

}
