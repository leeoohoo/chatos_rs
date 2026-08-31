@preconcurrency import AppKit
import Foundation

@MainActor
enum NativeSystemPermissionOnboarding {
    private static var controller: NativeSystemPermissionOnboardingWindowController?

    static func present(permissionID: String) {
        let controller = controller ?? NativeSystemPermissionOnboardingWindowController()
        self.controller = controller
        controller.present(permissionID: permissionID)
    }
}

@MainActor
private final class NativeSystemPermissionOnboardingWindowController:
    NSWindowController,
    NSWindowDelegate
{
    private let titleLabel = NSTextField(labelWithString: "")
    private let purposeLabel = NSTextField(wrappingLabelWithString: "")
    private let statusLabel = NSTextField(wrappingLabelWithString: "")
    private let settingsLabel = NSTextField(labelWithString: "")
    private let targetView = NativeSystemPermissionTargetView()
    private var permissionID = ""
    private var refreshTimer: Timer?

    init() {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 660, height: 500),
            styleMask: [.titled, .closable, .miniaturizable],
            backing: .buffered,
            defer: false
        )
        window.title = "ChatOS 系统授权引导"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false
        window.level = .floating
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        window.center()

        super.init(window: window)
        window.delegate = self
        configureContent(in: window)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    func present(permissionID: String) {
        self.permissionID = permissionID
        refreshUI()
        startRefreshTimer()
        showWindow(nil)
        window?.center()
        window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func windowWillClose(_ notification: Notification) {
        refreshTimer?.invalidate()
        refreshTimer = nil
    }

    private func configureContent(in window: NSWindow) {
        let background = NSVisualEffectView()
        background.material = .contentBackground
        background.blendingMode = .behindWindow
        background.state = .active
        window.contentView = background

        let iconContainer = NSView()
        iconContainer.wantsLayer = true
        iconContainer.layer?.cornerRadius = 30
        iconContainer.layer?.backgroundColor = NSColor.systemBlue.withAlphaComponent(0.12).cgColor
        iconContainer.translatesAutoresizingMaskIntoConstraints = false

        let icon = NSImageView()
        icon.image = NSImage(
            systemSymbolName: "hand.raised.square.fill",
            accessibilityDescription: "系统授权"
        )
        icon.symbolConfiguration = NSImage.SymbolConfiguration(pointSize: 28, weight: .semibold)
        icon.contentTintColor = .systemBlue
        icon.translatesAutoresizingMaskIntoConstraints = false
        iconContainer.addSubview(icon)

        titleLabel.font = .systemFont(ofSize: 26, weight: .bold)
        titleLabel.alignment = .center

        purposeLabel.font = .systemFont(ofSize: 14)
        purposeLabel.textColor = .secondaryLabelColor
        purposeLabel.alignment = .center
        purposeLabel.maximumNumberOfLines = 2

        settingsLabel.font = .systemFont(ofSize: 13, weight: .semibold)
        settingsLabel.textColor = .secondaryLabelColor
        settingsLabel.alignment = .center

        targetView.translatesAutoresizingMaskIntoConstraints = false

        statusLabel.font = .systemFont(ofSize: 13, weight: .semibold)
        statusLabel.alignment = .center
        statusLabel.maximumNumberOfLines = 2

        let openButton = NSButton(
            title: "打开系统设置",
            target: self,
            action: #selector(openSystemSettings)
        )
        openButton.bezelStyle = .rounded
        openButton.controlSize = .large

        let revealButton = NSButton(
            title: "在访达中显示 ChatOS",
            target: self,
            action: #selector(revealTarget)
        )
        revealButton.bezelStyle = .rounded
        revealButton.controlSize = .large

        let buttonRow = NSStackView(views: [openButton, revealButton])
        buttonRow.orientation = .horizontal
        buttonRow.alignment = .centerY
        buttonRow.spacing = 10

        let contentStack = NSStackView(views: [
            iconContainer,
            titleLabel,
            purposeLabel,
            settingsLabel,
            targetView,
            statusLabel,
            buttonRow,
        ])
        contentStack.orientation = .vertical
        contentStack.alignment = .centerX
        contentStack.spacing = 12
        contentStack.translatesAutoresizingMaskIntoConstraints = false
        background.addSubview(contentStack)

        NSLayoutConstraint.activate([
            contentStack.leadingAnchor.constraint(equalTo: background.leadingAnchor, constant: 42),
            contentStack.trailingAnchor.constraint(equalTo: background.trailingAnchor, constant: -42),
            contentStack.topAnchor.constraint(equalTo: background.topAnchor, constant: 28),
            contentStack.bottomAnchor.constraint(lessThanOrEqualTo: background.bottomAnchor, constant: -24),
            iconContainer.widthAnchor.constraint(equalToConstant: 60),
            iconContainer.heightAnchor.constraint(equalToConstant: 60),
            icon.centerXAnchor.constraint(equalTo: iconContainer.centerXAnchor),
            icon.centerYAnchor.constraint(equalTo: iconContainer.centerYAnchor),
            targetView.widthAnchor.constraint(equalTo: contentStack.widthAnchor),
            targetView.heightAnchor.constraint(equalToConstant: 122),
        ])
    }

    private func startRefreshTimer() {
        refreshTimer?.invalidate()
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.refreshUI()
            }
        }
    }

    private func refreshUI() {
        let granted = NativeSystemPermissions.isGranted(permissionID)
        titleLabel.stringValue = "允许 ChatOS 使用\(permissionName(permissionID))"
        purposeLabel.stringValue = NativeSystemPermissions.permissionPurpose(permissionID)
        settingsLabel.stringValue = NativeSystemPermissions.settingsTitle(permissionID)
        targetView.update(targetURL: Self.authorizationTargetURL)
        statusLabel.stringValue = granted
            ? "✓ 已检测到权限，可以返回 ChatOS 继续使用。"
            : "把下方 ChatOS App 拖入系统设置应用列表并开启；完成后这里会自动更新。"
        statusLabel.textColor = granted ? .systemGreen : .systemOrange
    }

    @objc
    private func openSystemSettings() {
        NativeSystemPermissions.requestAndOpenSettings(permissionID)
    }

    @objc
    private func revealTarget() {
        NSWorkspace.shared.activateFileViewerSelecting([Self.authorizationTargetURL])
    }

    private func permissionName(_ id: String) -> String {
        switch id {
        case "accessibility": "辅助功能"
        case "screen_recording": "屏幕录制权限"
        case "full_disk_access": "完全磁盘访问"
        default: "系统权限"
        }
    }

    fileprivate static var authorizationTargetURL: URL {
        let bundleURL = Bundle.main.bundleURL.standardizedFileURL
        if bundleURL.pathExtension.lowercased() == "app" {
            return bundleURL
        }
        var candidate = URL(fileURLWithPath: CommandLine.arguments[0]).standardizedFileURL
        while candidate.pathComponents.count > 1 {
            if candidate.pathExtension.lowercased() == "app" { return candidate }
            candidate.deleteLastPathComponent()
        }
        return URL(fileURLWithPath: CommandLine.arguments[0]).standardizedFileURL
    }
}

@MainActor
private final class NativeSystemPermissionTargetView: NSView, NSDraggingSource {
    private let iconView = NSImageView()
    private let titleLabel = NSTextField(labelWithString: "拖动 ChatOS 到系统设置应用列表")
    private let detailLabel = NSTextField(
        wrappingLabelWithString: "按住左侧 App 图标，直接拖到系统设置当前权限页面；无需点击“+”后再查找程序。"
    )
    private var targetURL = NativeSystemPermissionOnboardingWindowController.authorizationTargetURL

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.cornerRadius = 14
        layer?.backgroundColor = NSColor.systemBlue.withAlphaComponent(0.07).cgColor
        layer?.borderWidth = 1.5
        layer?.borderColor = NSColor.systemBlue.withAlphaComponent(0.35).cgColor

        iconView.imageScaling = .scaleProportionallyUpOrDown
        iconView.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.font = .systemFont(ofSize: 15, weight: .semibold)
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        detailLabel.font = .systemFont(ofSize: 12)
        detailLabel.textColor = .secondaryLabelColor
        detailLabel.maximumNumberOfLines = 2
        detailLabel.translatesAutoresizingMaskIntoConstraints = false

        addSubview(iconView)
        addSubview(titleLabel)
        addSubview(detailLabel)
        NSLayoutConstraint.activate([
            iconView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            iconView.centerYAnchor.constraint(equalTo: centerYAnchor),
            iconView.widthAnchor.constraint(equalToConstant: 66),
            iconView.heightAnchor.constraint(equalToConstant: 66),
            titleLabel.leadingAnchor.constraint(equalTo: iconView.trailingAnchor, constant: 15),
            titleLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            titleLabel.topAnchor.constraint(equalTo: topAnchor, constant: 28),
            detailLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            detailLabel.trailingAnchor.constraint(equalTo: titleLabel.trailingAnchor),
            detailLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 7),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    func update(targetURL: URL) {
        self.targetURL = targetURL
        iconView.image = NSWorkspace.shared.icon(forFile: targetURL.path)
        toolTip = "拖动 \(targetURL.lastPathComponent) 到系统设置"
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
    }

    override func mouseDragged(with event: NSEvent) {
        let item = NSPasteboardItem()
        item.setString(targetURL.absoluteString, forType: .fileURL)
        let draggingItem = NSDraggingItem(pasteboardWriter: item)
        let dragImage = iconView.image ?? NSWorkspace.shared.icon(forFile: targetURL.path)
        draggingItem.setDraggingFrame(
            NSRect(origin: convert(event.locationInWindow, from: nil), size: NSSize(width: 72, height: 72)),
            contents: dragImage
        )
        beginDraggingSession(with: [draggingItem], event: event, source: self)
    }

    func draggingSession(
        _ session: NSDraggingSession,
        sourceOperationMaskFor context: NSDraggingContext
    ) -> NSDragOperation {
        .copy
    }
}
