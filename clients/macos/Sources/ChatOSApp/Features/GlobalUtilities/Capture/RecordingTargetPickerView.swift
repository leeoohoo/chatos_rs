import ChatOSConnector
import SwiftUI

@MainActor
final class RecordingTargetPickerViewModel: ObservableObject {
    @Published private(set) var targets: [NativeScreenRecordingTarget] = []
    @Published var selectedID: String?
    @Published var capturesSystemAudio = false
    @Published private(set) var isLoading = false
    @Published private(set) var errorMessage: String?

    var onStart: ((NativeScreenRecordingTarget, Bool) -> Void)?
    var onCancel: (() -> Void)?

    private let service: NativeScreenRecordingService

    init(service: NativeScreenRecordingService) {
        self.service = service
    }

    var selectedTarget: NativeScreenRecordingTarget? {
        targets.first(where: { $0.id == selectedID })
    }

    func load() {
        isLoading = true
        errorMessage = nil
        Task { [weak self, service] in
            do {
                let values = try await service.availableTargets()
                self?.targets = values
                self?.selectedID = values.first?.id
            } catch {
                self?.errorMessage = error.localizedDescription
            }
            self?.isLoading = false
        }
    }

    func start() {
        guard let selectedTarget else { return }
        onStart?(selectedTarget, capturesSystemAudio)
    }
}

struct RecordingTargetPickerView: View {
    @ObservedObject var viewModel: RecordingTargetPickerViewModel
    let isEnglish: Bool

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Image(systemName: "record.circle")
                    .font(.system(size: 23, weight: .semibold))
                    .foregroundStyle(.red)
                VStack(alignment: .leading, spacing: 2) {
                    Text(isEnglish ? "Screen Recording" : "录屏")
                        .font(.system(size: 19, weight: .semibold))
                    Text(isEnglish ? "Choose an entire display or one window" : "选择整个显示器或单个窗口")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }
            .padding(.horizontal, 20)
            .frame(height: 72)
            Divider()

            if viewModel.isLoading {
                ProgressView(isEnglish ? "Loading recording targets…" : "正在读取录制目标…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error = viewModel.errorMessage {
                ContentUnavailableView(
                    isEnglish ? "Targets Unavailable" : "无法读取录制目标",
                    systemImage: "exclamationmark.triangle",
                    description: Text(error)
                )
            } else {
                List(selection: $viewModel.selectedID) {
                    if !displayTargets.isEmpty {
                        Section(isEnglish ? "Entire Displays" : "整个显示器（切换应用也会录制）") {
                            targetRows(displayTargets)
                        }
                    }
                    if !windowTargets.isEmpty {
                        Section(isEnglish ? "Applications" : "应用（录制主窗口）") {
                            targetRows(windowTargets)
                        }
                    }
                }
                .listStyle(.inset)
            }

            Divider()
            HStack(spacing: 14) {
                Toggle(
                    isEnglish ? "Record system audio" : "录制系统声音",
                    isOn: $viewModel.capturesSystemAudio
                )
                .toggleStyle(.switch)
                Spacer()
                Button(isEnglish ? "Cancel" : "取消") {
                    viewModel.onCancel?()
                }
                .keyboardShortcut(.cancelAction)
                Button(isEnglish ? "Start Recording" : "开始录制") {
                    viewModel.start()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(viewModel.selectedTarget == nil)
                .buttonStyle(.borderedProminent)
                .tint(.red)
            }
            .padding(.horizontal, 18)
            .frame(height: 58)
        }
        .frame(width: 620, height: 500)
        .background(.ultraThickMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .strokeBorder(.white.opacity(0.16), lineWidth: 1)
        }
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private var displayTargets: [NativeScreenRecordingTarget] {
        viewModel.targets.filter { $0.kind == .display }
    }

    private var windowTargets: [NativeScreenRecordingTarget] {
        viewModel.targets.filter { $0.kind == .window }
    }

    @ViewBuilder
    private func targetRows(_ targets: [NativeScreenRecordingTarget]) -> some View {
        ForEach(targets) { target in
            HStack(spacing: 11) {
                Image(systemName: target.kind == .display ? "display" : "macwindow")
                    .foregroundStyle(target.kind == .display ? .blue : .secondary)
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: 2) {
                    Text(targetTitle(target)).lineLimit(1)
                    if let subtitle = targetSubtitle(target) {
                        Text(subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
            }
            .tag(Optional(target.id))
        }
    }

    private func targetTitle(_ target: NativeScreenRecordingTarget) -> String {
        guard target.kind == .display,
              let index = displayTargets.firstIndex(where: { $0.id == target.id }) else {
            return target.title
        }
        return isEnglish ? "Entire Display \(index + 1)" : "整个显示器 \(index + 1)"
    }

    private func targetSubtitle(_ target: NativeScreenRecordingTarget) -> String? {
        if target.kind == .display {
            return isEnglish
                ? "Records everything shown here, including app switching · \(target.subtitle ?? "")"
                : "录制这个屏幕上显示的全部内容，切换应用也会继续 · \(target.subtitle ?? "")"
        }
        return target.subtitle
    }
}
