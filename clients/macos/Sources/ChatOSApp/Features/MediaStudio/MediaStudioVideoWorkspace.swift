import AppKit
import ChatOSCore
import SwiftUI
import UniformTypeIdentifiers

extension MediaStudioView {
    var videoWorkspace: some View {
        HStack(alignment: .top, spacing: 18) {
            videoResultSurface
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            videoSettingsSurface
                .frame(width: 356)
        }
        .padding(20)
    }

    var videoResultSurface: some View {
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

    var videoCanvasSubtitle: String {
        switch viewModel.videoCanvasState {
        case let .progress(progress): return videoProgressText(progress)
        case .failed: return appModel.localized("生成失败", english: "Failed")
        case let .result(latest):
            return "\(latest.modelName) · " + appModel.localized("已保存到本机", english: "Saved locally")
        case .empty:
            return appModel.localized("视频结果不会进入聊天记录", english: "Video results stay outside chat history")
        }
    }

    func videoProgressSurface(_ progress: VideoGenerationProgress) -> some View {
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

    func videoProgressText(_ progress: VideoGenerationProgress) -> String {
        let status: String
        switch progress.status {
        case "submitting": status = appModel.localized("提交中", english: "Submitting")
        case "uploading": status = appModel.localized("上传素材中", english: "Uploading media")
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

    var videoSettingsSurface: some View {
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
                videoReferenceAudioSelector

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
                        fieldLabel((viewModel.videoProfile.isMiniMax || viewModel.videoProfile.isSeedance)
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

                if viewModel.videoProfile.isMiniMax || viewModel.videoProfile.isSeedance {
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
    var videoModelSelector: some View {
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
                        "当前配置中没有视频模型。添加 Seedance、MiniMax H3 或其他兼容视频模型后刷新。",
                        english: "No video models are configured. Add Seedance, MiniMax H3, or another compatible video model, then refresh."
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

    var videoInputImageSelector: some View {
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

    func chooseVideoInputImage() {
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

    var videoReferenceAudioSelector: some View {
        VStack(alignment: .leading, spacing: 8) {
            fieldLabel(appModel.localized("参考音频（可选）", english: "Reference Audio (Optional)"))
            if viewModel.isLoadingVideoReferenceAudio {
                ProgressView(appModel.localized("正在读取参考音频…", english: "Loading reference audio…"))
                    .font(.caption)
                    .controlSize(.small)
            }
            if let audio = viewModel.videoReferenceAudio {
                HStack(spacing: 10) {
                    Image(systemName: "waveform.circle.fill")
                        .font(.system(size: 28))
                        .foregroundStyle(.purple)
                    Text(audio.name)
                        .font(.system(size: 11.5, weight: .medium))
                        .lineLimit(1)
                    Spacer()
                    Button { chooseVideoReferenceAudio() } label: {
                        Image(systemName: "arrow.triangle.2.circlepath")
                    }
                    .buttonStyle(.borderless)
                    Button { viewModel.removeVideoReferenceAudio() } label: {
                        Image(systemName: "xmark")
                    }
                    .buttonStyle(.borderless)
                }
                .padding(9)
                .background(Color.purple.opacity(0.055), in: RoundedRectangle(cornerRadius: 11))
            } else {
                Button { chooseVideoReferenceAudio() } label: {
                    Label(
                        appModel.localized("添加参考音频", english: "Add reference audio"),
                        systemImage: "waveform.badge.plus"
                    )
                    .font(.system(size: 11.5, weight: .medium))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 11)
                }
                .buttonStyle(.plain)
                .background(Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 11))
                .overlay {
                    RoundedRectangle(cornerRadius: 11)
                        .stroke(Color.primary.opacity(0.11), style: StrokeStyle(lineWidth: 1, dash: [5, 4]))
                }
            }
        }
    }

    func chooseVideoReferenceAudio() {
        let panel = NSOpenPanel()
        panel.title = appModel.localized("选择视频参考音频", english: "Choose Video Reference Audio")
        panel.prompt = appModel.localized("选择", english: "Choose")
        panel.allowedContentTypes = [.audio]
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        viewModel.selectVideoReferenceAudio(from: url)
    }

}
