import SwiftUI

struct PetSettingsView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var preferences: PetPreferencesStore

    var body: some View {
        SettingsGroupedPage {
            globalPetCard
            notificationsCard
            appearanceCard
        }
    }

    private var globalPetCard: some View {
        LocalConnectorCard(
            model.localized("全局展示", english: "Global Presence"),
            subtitle: model.localized(
                "让宠物脱离主窗口，在其他桌面与应用上持续展示任务状态。",
                english: "Keep the pet visible outside the main window to show task status across apps and desktops."
            ),
            systemImage: "pawprint.fill"
        ) {
            VStack(spacing: 0) {
                petToggleRow(
                    title: model.localized("启用全局宠物", english: "Enable global pet"),
                    detail: model.localized(
                        "关闭后宠物窗口会立即隐藏，但提醒记录仍会保留。",
                        english: "Hides the pet window immediately while preserving notification history."
                    ),
                    isOn: $preferences.isEnabled
                )
                Divider().padding(.vertical, 12)
                petToggleRow(
                    title: model.localized(
                        "跨桌面与全屏应用显示",
                        english: "Show across desktops and full-screen apps"
                    ),
                    detail: model.localized(
                        "切换 Space 或进入全屏应用时，宠物仍保持可见。",
                        english: "Keeps the pet visible when switching Spaces or entering full-screen apps."
                    ),
                    isOn: $preferences.showAcrossSpaces
                )
            }
        }
    }

    private var notificationsCard: some View {
        LocalConnectorCard(
            model.localized("事件提醒", english: "Event Notifications"),
            subtitle: model.localized(
                "决定哪些任务事件会主动出现在宠物旁边。",
                english: "Choose which task events appear proactively beside the pet."
            ),
            systemImage: "bell.badge.fill"
        ) {
            VStack(spacing: 0) {
                petToggleRow(
                    title: model.localized("展示 AI 执行过程", english: "Show AI execution progress"),
                    detail: model.localized(
                        "显示正在执行的任务、关键步骤和可取消状态。",
                        english: "Shows active tasks, important steps, and cancellation state."
                    ),
                    isOn: $preferences.showProcess
                )
                Divider().padding(.vertical, 12)
                petToggleRow(
                    title: model.localized("展示任务完成反馈", english: "Show task completion feedback"),
                    detail: model.localized(
                        "任务完成后保留明确反馈，并允许继续查看详情。",
                        english: "Keeps a clear completion result and lets you open the details."
                    ),
                    isOn: $preferences.showCompletions
                )
            }
        }
    }

    private var appearanceCard: some View {
        LocalConnectorCard(
            model.localized("外观与位置", english: "Appearance & Position"),
            subtitle: model.localized(
                "调整宠物尺寸，或把它恢复到默认屏幕位置。",
                english: "Adjust the pet size or restore its default screen position."
            ),
            systemImage: "slider.horizontal.3"
        ) {
            VStack(spacing: 14) {
                HStack(spacing: 16) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(model.localized("宠物大小", english: "Pet size"))
                            .appFont(.headline)
                        Text(model.localized(
                            "只改变宠物本体尺寸，消息卡片会保持易读。",
                            english: "Changes only the pet itself; message cards remain readable."
                        ))
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 20)
                    Image(systemName: "pawprint")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                    Slider(value: $preferences.size, in: 72...180, step: 4)
                        .frame(width: 240)
                    Image(systemName: "pawprint.fill")
                        .appFont(.title3)
                        .foregroundStyle(.secondary)
                    Text("\(Int(preferences.size))")
                        .appFont(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .frame(width: 36, alignment: .trailing)
                }
                Divider()
                HStack(spacing: 12) {
                    Image(systemName: "shippingbox.fill")
                        .foregroundStyle(.tint)
                        .frame(width: 32, height: 32)
                        .background(.tint.opacity(0.1), in: RoundedRectangle(cornerRadius: 9))
                    VStack(alignment: .leading, spacing: 3) {
                        Text(model.localized("当前宠物资源", english: "Current pet asset"))
                            .appFont(.headline)
                        Text(model.localized("枫团 · 秋日小狐狸", english: "Fengtuan · Autumn Fox"))
                            .appFont(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button(model.localized("恢复默认位置", english: "Reset Position")) {
                        preferences.requestPositionReset()
                    }
                }
            }
        }
    }

    private func petToggleRow(
        title: String,
        detail: String,
        isOn: Binding<Bool>
    ) -> some View {
        HStack(alignment: .center, spacing: 20) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title).appFont(.headline)
                Text(detail)
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 20)
            Toggle("", isOn: isOn)
                .labelsHidden()
                .toggleStyle(.switch)
        }
    }
}
