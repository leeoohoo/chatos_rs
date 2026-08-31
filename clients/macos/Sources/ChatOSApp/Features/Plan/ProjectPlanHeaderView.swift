import SwiftUI

struct ProjectPlanHeaderView: View {
    @ObservedObject var viewModel: ProjectPlanViewModel

    var body: some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 3) {
                Text("Plan")
                    .appFont(.headline)
                Text(summary)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Button {
                Task { await viewModel.load() }
            } label: {
                Label("刷新", systemImage: "arrow.clockwise")
            }
            .disabled(viewModel.isLoading)
        }
        .padding(.horizontal, 16)
        .frame(height: 58)
        .background(Color(nsColor: .windowBackgroundColor))
        .overlay(alignment: .bottom) { Divider() }
    }

    private var summary: String {
        let counts = viewModel.snapshot?.counts
        return "\(viewModel.requirements.count) 个需求 · \(counts?.total ?? 0) 个项目任务 · \(counts?.open ?? 0) 个未完成"
    }
}

struct ProjectPlanErrorBanner: View {
    let message: String
    let onRetry: () -> Void
    let onDismiss: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.red)
            Text(message)
                .appFont(.caption)
                .foregroundStyle(.red)
                .lineLimit(2)
            Spacer(minLength: 12)
            Button("重试", action: onRetry)
            Button(action: onDismiss) {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .help("关闭")
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 9)
        .background(Color.red.opacity(0.08))
        .overlay(alignment: .bottom) { Divider() }
    }
}
