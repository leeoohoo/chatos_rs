@preconcurrency import AppKit
import Foundation

@MainActor
enum PermissionOnboarding {
    private static var controller: PermissionOnboardingWindowController?

    static func present(requestedPermissions: Set<MacPermissionKind>) {
        let effectivePermissions = requestedPermissions.isEmpty
            ? Set(MacPermissionKind.allCases)
            : requestedPermissions
        let controller = controller ?? PermissionOnboardingWindowController()
        self.controller = controller
        controller.present(permissions: effectivePermissions, standalone: false)
    }

    static func runStandalone() {
        let application = NSApplication.shared
        application.setActivationPolicy(.accessory)
        let controller = controller ?? PermissionOnboardingWindowController()
        self.controller = controller
        controller.present(
            permissions: Set(MacPermissionKind.allCases),
            standalone: true
        )
        application.run()
    }
}

@MainActor
private final class PermissionOnboardingWindowController:
    NSWindowController,
    NSWindowDelegate
{
    private let contentStack = NSStackView()
    private let permissionStack = NSStackView()
    private let authorizationTargetView = AuthorizationTargetDragView()
    private let completionLabel = NSTextField(wrappingLabelWithString: "")
    private var refreshTimer: Timer?
    private var requestedPermissions = Set(MacPermissionKind.allCases)
    private var requestedAt: [MacPermissionKind: Date] = [:]
    private var standalone = false

    init() {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 680, height: 540),
            styleMask: [.titled, .closable, .miniaturizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Visual Computer Use 权限"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false
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

    func present(
        permissions: Set<MacPermissionKind>,
        standalone: Bool
    ) {
        requestedPermissions = permissions
        self.standalone = standalone
        refreshUI()
        startRefreshTimer()

        NSApplication.shared.setActivationPolicy(.accessory)
        showWindow(nil)
        window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func windowWillClose(_ notification: Notification) {
        refreshTimer?.invalidate()
        refreshTimer = nil
        if standalone {
            NSApp.terminate(nil)
        }
    }

    private func configureContent(in window: NSWindow) {
        let background = NSVisualEffectView()
        background.material = .contentBackground
        background.blendingMode = .behindWindow
        background.state = .active
        background.translatesAutoresizingMaskIntoConstraints = false
        window.contentView = background

        contentStack.orientation = .vertical
        contentStack.alignment = .centerX
        contentStack.spacing = 12
        contentStack.translatesAutoresizingMaskIntoConstraints = false
        background.addSubview(contentStack)

        let iconContainer = NSView()
        iconContainer.wantsLayer = true
        iconContainer.layer?.cornerRadius = 34
        iconContainer.layer?.backgroundColor = NSColor.systemBlue.withAlphaComponent(0.12).cgColor
        iconContainer.translatesAutoresizingMaskIntoConstraints = false

        let icon = NSImageView()
        icon.image = NSImage(
            systemSymbolName: "cursorarrow.motionlines",
            accessibilityDescription: "Visual Computer Use"
        )
        icon.symbolConfiguration = NSImage.SymbolConfiguration(
            pointSize: 32,
            weight: .semibold
        )
        icon.contentTintColor = .systemBlue
        icon.translatesAutoresizingMaskIntoConstraints = false
        iconContainer.addSubview(icon)

        let title = NSTextField(
            labelWithString: "允许 Visual Computer Use 控制这台 Mac"
        )
        title.font = .systemFont(ofSize: 28, weight: .bold)
        title.alignment = .center

        let subtitle = NSTextField(
            wrappingLabelWithString:
                "只有在你明确要求执行任务时，才会使用下面的权限。\n截图用于视觉判断，辅助功能用于发送真实鼠标和键盘事件。"
        )
        subtitle.font = .systemFont(ofSize: 14, weight: .regular)
        subtitle.textColor = .secondaryLabelColor
        subtitle.alignment = .center
        subtitle.maximumNumberOfLines = 2

        permissionStack.orientation = .vertical
        permissionStack.alignment = .centerX
        permissionStack.spacing = 12

        let revealButton = NSButton(
            title: "找不到？在访达中显示",
            target: self,
            action: #selector(revealAuthorizationTarget)
        )
        revealButton.bezelStyle = .rounded
        revealButton.controlSize = .regular
        revealButton.translatesAutoresizingMaskIntoConstraints = false

        authorizationTargetView.translatesAutoresizingMaskIntoConstraints = false
        authorizationTargetView.addSubview(revealButton)

        completionLabel.font = .systemFont(ofSize: 13, weight: .semibold)
        completionLabel.alignment = .center
        completionLabel.maximumNumberOfLines = 2

        contentStack.addArrangedSubview(iconContainer)
        contentStack.addArrangedSubview(title)
        contentStack.addArrangedSubview(subtitle)
        contentStack.addArrangedSubview(permissionStack)
        contentStack.addArrangedSubview(authorizationTargetView)
        contentStack.addArrangedSubview(completionLabel)
        contentStack.setCustomSpacing(16, after: iconContainer)
        contentStack.setCustomSpacing(7, after: title)
        contentStack.setCustomSpacing(20, after: subtitle)
        contentStack.setCustomSpacing(16, after: permissionStack)

        NSLayoutConstraint.activate([
            contentStack.leadingAnchor.constraint(equalTo: background.leadingAnchor, constant: 42),
            contentStack.trailingAnchor.constraint(equalTo: background.trailingAnchor, constant: -42),
            contentStack.topAnchor.constraint(equalTo: background.topAnchor, constant: 30),
            contentStack.bottomAnchor.constraint(lessThanOrEqualTo: background.bottomAnchor, constant: -24),

            iconContainer.widthAnchor.constraint(equalToConstant: 68),
            iconContainer.heightAnchor.constraint(equalToConstant: 68),
            icon.centerXAnchor.constraint(equalTo: iconContainer.centerXAnchor),
            icon.centerYAnchor.constraint(equalTo: iconContainer.centerYAnchor),

            permissionStack.widthAnchor.constraint(equalTo: contentStack.widthAnchor),
            authorizationTargetView.widthAnchor.constraint(equalTo: contentStack.widthAnchor),
            authorizationTargetView.heightAnchor.constraint(greaterThanOrEqualToConstant: 104),
            revealButton.trailingAnchor.constraint(equalTo: authorizationTargetView.trailingAnchor, constant: -14),
            revealButton.bottomAnchor.constraint(equalTo: authorizationTargetView.bottomAnchor, constant: -12),
        ])
    }

    private func startRefreshTimer() {
        refreshTimer?.invalidate()
        refreshTimer = Timer.scheduledTimer(
            withTimeInterval: 0.45,
            repeats: true
        ) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.refreshUI()
            }
        }
    }

    private func refreshUI() {
        permissionStack.arrangedSubviews.forEach { view in
            permissionStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }

        for permission in MacPermissionKind.allCases
        where requestedPermissions.contains(permission) {
            let card = PermissionCardView(
                permission: permission,
                granted: permission.isGranted(),
                restartSuggested: restartSuggested(for: permission)
            ) { [weak self] requestedPermission in
                self?.requestedAt[requestedPermission] = Date()
                requestedPermission.requestAndOpenSettings()
                self?.refreshUI()
            }
            permissionStack.addArrangedSubview(card)
            card.widthAnchor.constraint(equalTo: permissionStack.widthAnchor).isActive = true
        }

        authorizationTargetView.update(
            targetURL: PermissionSupport.authorizationTargetURL,
            isAppBundle: PermissionSupport.appBundleURL != nil
        )

        let allRequestedGranted = requestedPermissions.allSatisfy { $0.isGranted() }
        if allRequestedGranted {
            completionLabel.stringValue =
                "✓ 所需权限均已启用。请重新连接 MCP，确保 macOS 将新权限应用到当前进程。"
            completionLabel.textColor = .systemGreen
        } else if requestedAt.values.contains(where: {
            Date().timeIntervalSince($0) > 1.5
        }) {
            completionLabel.stringValue =
                "如果系统设置中的开关已经打开，但这里仍未更新，请重启或重新连接 MCP。"
            completionLabel.textColor = .systemOrange
        } else {
            completionLabel.stringValue =
                "点击“打开设置”，再把上方 App 图标直接拖进系统设置列表并启用。"
            completionLabel.textColor = .secondaryLabelColor
        }
    }

    private func restartSuggested(for permission: MacPermissionKind) -> Bool {
        guard !permission.isGranted(), let requestedAt = requestedAt[permission] else {
            return false
        }
        return Date().timeIntervalSince(requestedAt) > 1.5
    }

    @objc
    private func revealAuthorizationTarget() {
        PermissionSupport.revealAuthorizationTarget()
    }
}

@MainActor
private final class AuthorizationTargetDragView: NSView, NSDraggingSource {
    private let iconView = NSImageView()
    private let titleLabel = NSTextField(labelWithString: "")
    private let detailLabel = NSTextField(wrappingLabelWithString: "")
    private var targetURL = PermissionSupport.authorizationTargetURL

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

        detailLabel.font = .systemFont(ofSize: 12, weight: .regular)
        detailLabel.textColor = .secondaryLabelColor
        detailLabel.maximumNumberOfLines = 2
        detailLabel.translatesAutoresizingMaskIntoConstraints = false

        addSubview(iconView)
        addSubview(titleLabel)
        addSubview(detailLabel)

        NSLayoutConstraint.activate([
            iconView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            iconView.topAnchor.constraint(equalTo: topAnchor, constant: 15),
            iconView.widthAnchor.constraint(equalToConstant: 56),
            iconView.heightAnchor.constraint(equalToConstant: 56),

            titleLabel.leadingAnchor.constraint(equalTo: iconView.trailingAnchor, constant: 14),
            titleLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -14),
            titleLabel.topAnchor.constraint(equalTo: topAnchor, constant: 16),

            detailLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            detailLabel.trailingAnchor.constraint(equalTo: titleLabel.trailingAnchor),
            detailLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 5),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    func update(targetURL: URL, isAppBundle: Bool) {
        self.targetURL = targetURL
        iconView.image = Self.brandedDragIcon()
        titleLabel.stringValue = isAppBundle
            ? "拖动 Visual Computer Use 到左侧系统设置列表"
            : "拖动当前可执行文件到左侧系统设置列表"
        detailLabel.stringValue = isAppBundle
            ? "按住这个 App 图标，直接拖到“屏幕与系统音频录制”的应用列表中。"
            : "当前不是稳定的 .app 身份；可以拖入授权，但重新构建后可能需要再次授权。"
        toolTip = "拖动 \(targetURL.lastPathComponent) 到系统设置"
    }

    private static func brandedDragIcon() -> NSImage {
        let size = NSSize(width: 128, height: 128)
        let image = NSImage(size: size, flipped: false) { rect in
            NSColor.systemBlue.withAlphaComponent(0.13).setFill()
            NSBezierPath(ovalIn: rect.insetBy(dx: 4, dy: 4)).fill()

            guard let symbol = NSImage(
                systemSymbolName: "cursorarrow.motionlines",
                accessibilityDescription: "Visual Computer Use"
            )?.withSymbolConfiguration(
                NSImage.SymbolConfiguration(pointSize: 58, weight: .semibold)
            ) else {
                return true
            }
            let symbolSize = symbol.size
            let symbolRect = NSRect(
                x: rect.midX - symbolSize.width / 2,
                y: rect.midY - symbolSize.height / 2,
                width: symbolSize.width,
                height: symbolSize.height
            )
            NSColor.systemBlue.set()
            symbol.draw(in: symbolRect)
            return true
        }
        image.isTemplate = false
        return image
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
    }

    override func mouseDragged(with event: NSEvent) {
        let pasteboardItem = NSPasteboardItem()
        pasteboardItem.setString(targetURL.absoluteString, forType: .fileURL)

        let draggingItem = NSDraggingItem(pasteboardWriter: pasteboardItem)
        let dragImage = iconView.image ?? NSWorkspace.shared.icon(forFile: targetURL.path)
        let origin = convert(event.locationInWindow, from: nil)
        draggingItem.setDraggingFrame(
            NSRect(x: origin.x - 28, y: origin.y - 28, width: 56, height: 56),
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

@MainActor
private final class PermissionCardView: NSView {
    private let permission: MacPermissionKind
    private let onAllow: (MacPermissionKind) -> Void

    init(
        permission: MacPermissionKind,
        granted: Bool,
        restartSuggested: Bool,
        onAllow: @escaping (MacPermissionKind) -> Void
    ) {
        self.permission = permission
        self.onAllow = onAllow
        super.init(frame: .zero)

        wantsLayer = true
        layer?.cornerRadius = 16
        layer?.backgroundColor = NSColor.controlBackgroundColor.cgColor
        layer?.borderWidth = 1
        layer?.borderColor = NSColor.separatorColor.withAlphaComponent(0.55).cgColor
        translatesAutoresizingMaskIntoConstraints = false

        let iconBackground = NSView()
        iconBackground.wantsLayer = true
        iconBackground.layer?.cornerRadius = 23
        iconBackground.layer?.backgroundColor = NSColor.systemBlue.withAlphaComponent(0.11).cgColor
        iconBackground.translatesAutoresizingMaskIntoConstraints = false

        let icon = NSImageView()
        icon.image = NSImage(
            systemSymbolName: permission.symbolName,
            accessibilityDescription: permission.title
        )
        icon.symbolConfiguration = NSImage.SymbolConfiguration(
            pointSize: 20,
            weight: .semibold
        )
        icon.contentTintColor = .systemBlue
        icon.translatesAutoresizingMaskIntoConstraints = false
        iconBackground.addSubview(icon)

        let labels = NSStackView()
        labels.orientation = .vertical
        labels.alignment = .leading
        labels.spacing = 3
        labels.translatesAutoresizingMaskIntoConstraints = false

        let title = NSTextField(labelWithString: permission.title)
        title.font = .systemFont(ofSize: 17, weight: .semibold)

        let purpose = NSTextField(
            wrappingLabelWithString: restartSuggested
                ? "授权后需要重新连接 MCP 才能完全生效。"
                : permission.purpose
        )
        purpose.font = .systemFont(ofSize: 12.5, weight: .regular)
        purpose.textColor = restartSuggested ? .systemOrange : .secondaryLabelColor
        purpose.maximumNumberOfLines = 2

        labels.addArrangedSubview(title)
        labels.addArrangedSubview(purpose)

        let statusOrButton: NSView
        if granted {
            let status = NSTextField(labelWithString: "✓ 已允许")
            status.font = .systemFont(ofSize: 13, weight: .semibold)
            status.textColor = .systemGreen
            statusOrButton = status
        } else {
            let button = NSButton(
                title: restartSuggested ? "重新检查" : "打开设置",
                target: self,
                action: #selector(handleAllow)
            )
            button.bezelStyle = .rounded
            button.keyEquivalent = ""
            statusOrButton = button
        }
        statusOrButton.translatesAutoresizingMaskIntoConstraints = false

        addSubview(iconBackground)
        addSubview(labels)
        addSubview(statusOrButton)

        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: 86),
            iconBackground.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            iconBackground.centerYAnchor.constraint(equalTo: centerYAnchor),
            iconBackground.widthAnchor.constraint(equalToConstant: 46),
            iconBackground.heightAnchor.constraint(equalToConstant: 46),
            icon.centerXAnchor.constraint(equalTo: iconBackground.centerXAnchor),
            icon.centerYAnchor.constraint(equalTo: iconBackground.centerYAnchor),

            labels.leadingAnchor.constraint(equalTo: iconBackground.trailingAnchor, constant: 14),
            labels.centerYAnchor.constraint(equalTo: centerYAnchor),
            labels.trailingAnchor.constraint(lessThanOrEqualTo: statusOrButton.leadingAnchor, constant: -14),

            statusOrButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            statusOrButton.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    @objc
    private func handleAllow() {
        onAllow(permission)
    }
}
