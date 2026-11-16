import SwiftUI

struct AgentTeamHeader: View {
    let name: String
    let goal: String?
    @Binding var selectedSection: AgentTeamSection
    let isRunning: Bool
    let hasInterruptedRuns: Bool
    let isPausing: Bool
    let isStopping: Bool
    let onPause: () -> Void
    let onStop: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 14) {
                identity
                controls
            }
            sectionPicker
                .frame(maxWidth: 470)
                .frame(maxWidth: .infinity, alignment: .trailing)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
        .background(AppPalette.surface)
    }

    private var identity: some View {
        HStack(spacing: 11) {
            Image(systemName: "person.3.fill")
                .appFont(.headline)
                .foregroundStyle(AppPalette.ai)
                .frame(width: 34, height: 34)
                .background(AppPalette.aiSoft, in: RoundedRectangle(cornerRadius: 10))
            VStack(alignment: .leading, spacing: 3) {
                Text(name)
                    .appFont(.headline.weight(.semibold))
                    .lineLimit(1)
                if let goal, !goal.isEmpty {
                    Text(goal)
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            .frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var controls: some View {
        AgentTeamRunControls(
            isRunning: isRunning,
            hasInterruptedRuns: hasInterruptedRuns,
            isPausing: isPausing,
            isStopping: isStopping,
            onPause: onPause,
            onStop: onStop
        )
    }

    private var sectionPicker: some View {
        Picker("团队区域", selection: $selectedSection) {
            ForEach(AgentTeamSection.allCases) { section in
                Label(section.rawValue, systemImage: section.iconName)
                    .tag(section)
            }
        }
        .labelsHidden()
        .pickerStyle(.segmented)
    }
}

/// Run actions must retain their readable width even beside a long team goal.
struct AgentTeamRunControls: View {
    let isRunning: Bool
    let hasInterruptedRuns: Bool
    let isPausing: Bool
    let isStopping: Bool
    let onPause: () -> Void
    let onStop: () -> Void

    var body: some View {
        let canStop = isRunning || hasInterruptedRuns
        HStack(spacing: 10) {
            Button(action: onPause) {
                Label("暂停", systemImage: "pause.fill")
                    .appFont(.body)
                    .opacity(isPausing ? 0 : 1)
                    .overlay {
                        ProgressView()
                            .controlSize(.small)
                            .opacity(isPausing ? 1 : 0)
                    }
            }
            .opacity(isRunning ? 1 : 0)
            .disabled(!isRunning || isPausing || isStopping)
            .allowsHitTesting(isRunning)
            .accessibilityHidden(!isRunning)
            .help("暂停当前团队的 Agent")
            Button(role: .destructive, action: onStop) {
                Label("停止全部", systemImage: "stop.fill")
                    .appFont(.body)
                    .opacity(isStopping ? 0 : 1)
                    .overlay {
                        ProgressView()
                            .controlSize(.small)
                            .opacity(isStopping ? 1 : 0)
                    }
            }
            .opacity(canStop ? 1 : 0)
            .disabled(!canStop || isStopping)
            .allowsHitTesting(canStop)
            .accessibilityHidden(!canStop)
            .help("停止当前团队的全部 Agent")
        }
        .labelStyle(.titleAndIcon)
        .buttonStyle(.bordered)
        .fixedSize(horizontal: true, vertical: false)
    }
}
