import ChatOSCore
import SwiftUI

struct ProjectRunInstancesSection: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: ProjectRunSettingsViewModel
    @State private var expandedInstanceIDs: Set<String> = []

    var body: some View {
        SettingsCard(title: "运行实例", systemImage: "rectangle.stack") {
            Text("每次点击“启动新实例”都会在本机创建一个独立进程。展开实例即可查看持续更新的标准输出和错误日志。")
                .appFont(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            if viewModel.instances.isEmpty {
                ContentUnavailableView(
                    "还没有运行实例",
                    systemImage: "play.circle",
                    description: Text("在上方选择运行目标，然后点击“启动新实例”。")
                )
                .frame(minHeight: 120)
            } else {
                VStack(spacing: 10) {
                    ForEach(viewModel.instances) { instance in
                        instanceCard(instance)
                    }
                }
            }
        }
    }

    private func instanceCard(_ instance: ProjectRunInstance) -> some View {
        let isExpanded = Binding(
            get: { expandedInstanceIDs.contains(instance.id) },
            set: { expanded in
                if expanded { expandedInstanceIDs.insert(instance.id) }
                else { expandedInstanceIDs.remove(instance.id) }
            }
        )

        return VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                Image(systemName: instance.isRunning ? "waveform.path.ecg" : "terminal")
                    .foregroundStyle(instance.isRunning ? .green : statusColor(instance.status))
                    .frame(width: 22)

                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 8) {
                        Text(instance.name).appFont(.subheadline.weight(.semibold))
                        StatusCapsule(
                            title: localizedRunStatus(instance.status),
                            color: instance.isRunning ? .green : statusColor(instance.status)
                        )
                    }
                    HStack(spacing: 10) {
                        if let startedAt = instance.startedAt {
                            Text(startedAt, style: .time)
                        }
                        if let cwd = instance.cwd {
                            Text(cwd).lineLimit(1).truncationMode(.middle)
                        }
                    }
                    .appFont(.caption.monospaced())
                    .foregroundStyle(.secondary)
                }

                Spacer(minLength: 12)

                if instance.isRunning {
                    Button("停止", systemImage: "stop.fill") {
                        Task { await viewModel.stop(instanceID: instance.id) }
                    }
                    .disabled(viewModel.isMutating)
                }

                Button("删除", systemImage: "trash", role: .destructive) {
                    Task { await viewModel.deleteInstance(instanceID: instance.id) }
                }
                .disabled(viewModel.isMutating)
            }

            Button {
                withAnimation(.easeInOut(duration: 0.18)) {
                    isExpanded.wrappedValue.toggle()
                }
            } label: {
                HStack(spacing: 7) {
                    Image(systemName: "chevron.right")
                        .appFont(.caption.weight(.semibold))
                        .rotationEffect(.degrees(isExpanded.wrappedValue ? 90 : 0))
                    Text(isExpanded.wrappedValue
                         ? model.localized("收起运行日志", english: "Collapse Run Log")
                         : model.localized("展开运行日志", english: "Expand Run Log"))
                        .appFont(.subheadline.weight(.medium))
                    Spacer()
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            if isExpanded.wrappedValue {
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Label("实时运行日志", systemImage: "text.alignleft")
                            .appFont(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                        Spacer()
                        if let exitCode = instance.exitCode {
                            Text(model.localized("退出码 \(exitCode)", english: "Exit Code \(exitCode)"))
                                .appFont(.caption.monospacedDigit())
                                .foregroundStyle(exitCode == 0 ? Color.secondary : Color.red)
                        } else if instance.isRunning {
                            Label("每秒刷新", systemImage: "arrow.clockwise")
                                .appFont(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }

                    CodePreviewView(
                        content: instance.log?.isEmpty == false
                            ? instance.log!
                            : model.localized(
                                "进程已经启动，暂时还没有日志输出。",
                                english: "The process has started, but no log output is available yet."
                            ),
                        fileName: "run.log",
                        followsTail: instance.isRunning
                    )
                    .frame(minHeight: 240, maxHeight: 380)
                    .clipShape(RoundedRectangle(cornerRadius: 9))
                    .overlay { RoundedRectangle(cornerRadius: 9).stroke(.separator) }
                }
                .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .padding(14)
        .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 11))
        .overlay { RoundedRectangle(cornerRadius: 11).stroke(.separator.opacity(0.7)) }
    }

    private func localizedRunStatus(_ status: String) -> String {
        switch status.lowercased() {
        case "running": model.localized("运行中", english: "Running")
        case "starting": model.localized("启动中", english: "Starting")
        case "stopping": model.localized("正在停止", english: "Stopping")
        case "stopped", "exited": model.localized("已结束", english: "Ended")
        case "error", "failed": model.localized("运行失败", english: "Run Failed")
        case "idle": model.localized("空闲", english: "Idle")
        default: status
        }
    }

    private func statusColor(_ status: String) -> Color {
        switch status.lowercased() {
        case "error", "failed": .red
        case "starting", "stopping": .orange
        default: .secondary
        }
    }
}
