import SwiftUI

struct ReasoningLevelControl: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var conversation: ConversationSessionViewModel
    @State private var isPresented = false

    var body: some View {
        Button {
            isPresented.toggle()
        } label: {
            Text(model.localized(
                "推理 \(levelName(conversation.effectiveReasoningLevel))",
                english: "Reasoning \(englishLevelName(conversation.effectiveReasoningLevel))"
            ))
        }
        .buttonStyle(.borderedProminent)
        .tint(
            conversation.effectiveReasoningLevel == "none"
                ? AppPalette.idleControl
                : AppPalette.ai
        )
        .disabled(
            conversation.isUpdatingRuntimeSettings
                || conversation.reasoningLevels.isEmpty
        )
        .help(reasoningHelp)
        .popover(isPresented: $isPresented, arrowEdge: .bottom) {
            reasoningPopover
                .presentationCompactAdaptation(.popover)
        }
    }

    private var reasoningPopover: some View {
        VStack(spacing: 12) {
            HStack(alignment: .top, spacing: 10) {
                Spacer().frame(width: 24)
                VStack(spacing: 3) {
                    HStack(spacing: 3) {
                        Text(levelName(conversation.effectiveReasoningLevel))
                            .foregroundStyle(AppPalette.ai)
                        Image(systemName: "chevron.right")
                            .appFont(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    .appFont(.headline)
                    Text(conversation.selectedModelOption?.displayName ?? "")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                .frame(maxWidth: .infinity)

                Button {
                    conversation.resetReasoningLevel()
                } label: {
                    Image(systemName: "arrow.counterclockwise")
                        .frame(width: 24, height: 24)
                }
                .buttonStyle(.plain)
                .disabled(
                    conversation.isUpdatingRuntimeSettings
                        || conversation.effectiveReasoningLevel == conversation.defaultReasoningLevel
                )
                .help(model.localized(
                    "恢复该模型的默认推理等级",
                    english: "Restore this model's default reasoning level"
                ))
            }

            ReasoningLevelSlider(
                levels: conversation.reasoningLevels,
                selectedLevel: conversation.effectiveReasoningLevel,
                isEnabled: !conversation.isUpdatingRuntimeSettings,
                onSelect: conversation.setReasoningLevel
            )
            .frame(height: 28)

            Text(model.localized(
                "可选档位由当前模型的厂商能力决定",
                english: "Available levels are determined by the current model provider"
            ))
            .appFont(.caption2)
            .foregroundStyle(.secondary)
        }
        .padding(14)
        .frame(width: 270)
    }

    private var reasoningHelp: String {
        if conversation.reasoningLevels.isEmpty {
            return model.localized(
                "当前模型未启用推理能力",
                english: "Reasoning is not enabled for the current model"
            )
        }
        return model.localized(
            "设置当前模型的推理等级",
            english: "Set the reasoning level for the current model"
        )
    }

    private func levelName(_ level: String) -> String {
        switch level {
        case "none": "关"
        case "auto": "自动"
        case "minimal": "极低"
        case "low": "低"
        case "medium": "中"
        case "high": "高"
        case "xhigh": "极高"
        case "max": "最高"
        default: level
        }
    }

    private func englishLevelName(_ level: String) -> String {
        switch level {
        case "none": "Off"
        case "auto": "Auto"
        case "minimal": "Minimal"
        case "low": "Low"
        case "medium": "Medium"
        case "high": "High"
        case "xhigh": "Extra High"
        case "max": "Max"
        default: level
        }
    }
}

private struct ReasoningLevelSlider: View {
    let levels: [String]
    let selectedLevel: String
    let isEnabled: Bool
    let onSelect: (String) -> Void

    @State private var draftIndex = 0

    var body: some View {
        GeometryReader { proxy in
            let width = proxy.size.width
            let centerRange = max(width - knobDiameter, 0)
            let step = levels.count > 1 ? centerRange / CGFloat(levels.count - 1) : 0
            let knobOffset = CGFloat(draftIndex) * step
            let fillWidth = min(width, knobDiameter / 2 + knobOffset)

            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.secondary.opacity(0.18))
                Capsule()
                    .fill(AppPalette.ai.opacity(isEnabled ? 0.92 : 0.45))
                    .frame(width: fillWidth)

                HStack(spacing: 0) {
                    ForEach(levels.indices, id: \.self) { index in
                        Circle()
                            .fill(index <= draftIndex ? Color.white.opacity(0.3) : Color.secondary.opacity(0.4))
                            .frame(width: 4, height: 4)
                        if index < levels.count - 1 { Spacer() }
                    }
                }
                .padding(.horizontal, knobDiameter / 2)

                Circle()
                    .fill(.white)
                    .frame(width: knobDiameter, height: knobDiameter)
                    .shadow(color: .black.opacity(0.12), radius: 2, y: 1)
                    .offset(x: knobOffset)
            }
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { value in
                        guard isEnabled else { return }
                        draftIndex = index(at: value.location.x, width: width)
                    }
                    .onEnded { value in
                        guard isEnabled, !levels.isEmpty else { return }
                        let index = index(at: value.location.x, width: width)
                        draftIndex = index
                        onSelect(levels[index])
                    }
            )
        }
        .onAppear { synchronizeSelection() }
        .onChange(of: selectedLevel) { _, _ in synchronizeSelection() }
        .opacity(isEnabled ? 1 : 0.65)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Reasoning level")
        .accessibilityValue(selectedLevel)
        .accessibilityAdjustableAction { direction in
            guard isEnabled, !levels.isEmpty else { return }
            let delta = direction == .increment ? 1 : -1
            let next = min(max(draftIndex + delta, 0), levels.count - 1)
            draftIndex = next
            onSelect(levels[next])
        }
    }

    private let knobDiameter: CGFloat = 24

    private func synchronizeSelection() {
        draftIndex = levels.firstIndex(of: selectedLevel) ?? 0
    }

    private func index(at x: CGFloat, width: CGFloat) -> Int {
        guard levels.count > 1 else { return 0 }
        let centerRange = max(width - knobDiameter, 1)
        let centeredX = min(max(x - knobDiameter / 2, 0), centerRange)
        return min(
            max(Int((centeredX / centerRange * CGFloat(levels.count - 1)).rounded()), 0),
            levels.count - 1
        )
    }
}
