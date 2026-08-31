import AppKit
import SwiftUI

struct VisualSessionOverlayHost: View {
    @ObservedObject var store: VisualSessionPresentationStore
    let currentConversationID: String?

    @ViewBuilder
    var body: some View {
        if let presentation = store.selectedPresentation,
           presentation.ownerSessionID == currentConversationID {
            VisualSessionOverlay(
                session: presentation,
                position: (store.selectedIndex ?? 0) + 1,
                sessionCount: store.presentations.count
            )
        }
    }
}

struct VisualSessionOverlay: View {
    @EnvironmentObject private var model: AppModel
    let session: VisualSessionPresentation
    let position: Int
    let sessionCount: Int

    var body: some View {
        if session.isExpanded {
            VStack(spacing: 0) {
                HStack(spacing: 10) {
                    Image(systemName: operationIcon)
                        .foregroundStyle(.white)
                        .frame(width: 30, height: 30)
                        .background(iconColor, in: RoundedRectangle(cornerRadius: 8))

                    VStack(alignment: .leading, spacing: 2) {
                        Text(displayTitle)
                            .appFont(.caption.weight(.semibold))
                            .lineLimit(1)
                        HStack(spacing: 5) {
                            Circle().fill(.green).frame(width: 6, height: 6)
                            Text(activityDescription)
                                .appFont(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }

                    Spacer()

                    if sessionCount > 1 {
                        Button(action: model.selectPreviousVisualSession) {
                            Image(systemName: "chevron.left")
                        }
                        .buttonStyle(.plain)
                        .help(model.localized("上一个实时画面", english: "Previous live view"))

                        Text("\(position)/\(sessionCount)")
                            .appFont(.caption2.monospacedDigit())
                            .foregroundStyle(.secondary)

                        Button(action: model.selectNextVisualSession) {
                            Image(systemName: "chevron.right")
                        }
                        .buttonStyle(.plain)
                        .help(model.localized("下一个实时画面", english: "Next live view"))
                    }

                    Button(action: model.toggleVisualSession) {
                        Image(systemName: "chevron.down")
                    }
                    .buttonStyle(.plain)
                }
                .padding(10)

                Divider().opacity(0.25)

                ZStack {
                    Color(nsColor: .windowBackgroundColor)
                    if let frameImage {
                        Image(nsImage: frameImage)
                            .resizable()
                            .interpolation(.high)
                            .antialiased(true)
                            .scaledToFit()
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    } else {
                        VStack(spacing: 10) {
                            ProgressView()
                                .controlSize(.small)
                            Text(model.localized("正在建立实时画面…", english: "Establishing live view…"))
                                .appFont(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                .frame(height: imageAreaHeight)

                Divider().opacity(0.25)

                HStack {
                    Text(session.session.pluginDisplayName)
                        .lineLimit(1)
                    if sessionCount > 1 {
                        Text(model.localized(
                            "· \(sessionCount) 个活动画面",
                            english: "· \(sessionCount) active views"
                        ))
                    }
                    Spacer()
                    Label(model.localized("仅在本机显示", english: "Visible only on this Mac"), systemImage: "lock.fill")
                }
                .appFont(.caption2)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 10)
                .frame(height: 28)
            }
            .frame(width: overlayWidth)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
            .overlay {
                RoundedRectangle(cornerRadius: 14)
                    .stroke(.primary.opacity(0.12), lineWidth: 1)
            }
            .shadow(radius: 18, y: 8)
        } else {
            Button(action: model.toggleVisualSession) {
                HStack(spacing: 8) {
                    Circle().fill(.green).frame(width: 7, height: 7)
                    Image(systemName: operationIcon)
                    Text(collapsedTitle)
                        .appFont(.caption.weight(.semibold))
                        .lineLimit(1)
                    if sessionCount > 1 {
                        Text("\(position)/\(sessionCount)")
                            .appFont(.caption2.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                    Image(systemName: "chevron.up")
                        .appFont(.caption2)
                }
                .padding(.horizontal, 13)
                .frame(width: sessionCount > 1 ? 204 : 168, height: 44)
            }
            .buttonStyle(.plain)
            .background(.regularMaterial, in: Capsule())
            .overlay { Capsule().stroke(.primary.opacity(0.12), lineWidth: 1) }
            .shadow(radius: 12, y: 5)
        }
    }

    private var frameImage: NSImage? {
        session.session.frameData.flatMap(NSImage.init(data:))
    }

    private let overlayWidth: CGFloat = 376

    private var imageAreaHeight: CGFloat {
        guard let frameImage,
              frameImage.size.width > 0,
              frameImage.size.height > 0 else {
            return 178
        }

        let fittedHeight = overlayWidth * frameImage.size.height / frameImage.size.width
        return min(max(fittedHeight, 178), 260)
    }

    private var isBrowser: Bool {
        session.session.componentKey.localizedCaseInsensitiveContains("browser")
    }

    private var operationIcon: String {
        isBrowser ? "safari.fill" : "rectangle.inset.filled.and.person.filled"
    }

    private var iconColor: Color {
        isBrowser ? Color(red: 0.17, green: 0.48, blue: 0.92) : AppPalette.ai
    }

    private var collapsedTitle: String {
        isBrowser
            ? model.localized("浏览器操作", english: "Browser activity")
            : model.localized("电脑操作", english: "Computer activity")
    }

    private var displayTitle: String {
        session.session.owner.taskTitle?.trimmingCharacters(in: .whitespacesAndNewlines)
            .nonEmpty ?? session.session.title
    }

    private var activityDescription: String {
        if let target = session.session.targetApplication,
           !target.trimmingCharacters(in: CharacterSet.whitespacesAndNewlines).isEmpty {
            return isBrowser
                ? model.localized("正在浏览 \(target)", english: "Browsing \(target)")
                : model.localized("正在操作 \(target)", english: "Controlling \(target)")
        }
        return model.localized("正在本机运行", english: "Running on this Mac")
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}
