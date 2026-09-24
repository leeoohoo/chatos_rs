import AppKit
import ChatOSConnector
import CoreGraphics
import Foundation

@MainActor
final class ScreenshotCoordinator {
    private static let workflowGate = ScreenshotWorkflowGate()

    private weak var model: AppModel?
    private let captureService = NativeScreenCaptureService()
    private let toastController = CaptureResultToastController()

    private var selectionController: ScreenSelectionOverlayController?
    private var annotationController: ScreenshotInlineAnnotationController?
    private var longCaptureController: LongScreenshotCaptureController?
    private var captureTask: Task<Void, Never>?
    private var previousApplication: NSRunningApplication?
    private var selectedScreen: NSScreen?
    private(set) var isRunning = false

    init(model: AppModel) {
        self.model = model
    }

    func start() {
        guard Self.workflowGate.acquire(self) else { return }

        if NSWorkspace.shared.frontmostApplication?.bundleIdentifier
            != Bundle.main.bundleIdentifier {
            previousApplication = NSWorkspace.shared.frontmostApplication
        }

        guard ensureScreenCapturePermission() else {
            Self.workflowGate.release(self)
            restorePreviousApplication()
            return
        }

        isRunning = true
        freezeScreensAndPresentSelection()
    }

    private func freezeScreensAndPresentSelection() {
        // Freeze the desktop before presenting or interacting with any ChatOS
        // window. Popovers, menus, and other transient windows may disappear as
        // soon as focus changes or the user starts dragging the selection.
        let targets = NSScreen.screens.compactMap { screen -> NativeScreenCaptureRegion? in
            guard let displayID = screen.deviceDescription[
                NSDeviceDescriptionKey("NSScreenNumber")
            ] as? NSNumber else { return nil }
            let scale = screen.backingScaleFactor
            return NativeScreenCaptureRegion(
                displayID: CGDirectDisplayID(displayID.uint32Value),
                sourceRect: CGRect(origin: .zero, size: screen.frame.size),
                outputSize: CGSize(
                    width: screen.frame.width * scale,
                    height: screen.frame.height * scale
                )
            )
        }

        captureTask?.cancel()
        captureTask = Task { [weak self] in
            guard let self else { return }
            do {
                let captures = try await self.captureService.capture(regions: targets)
                try Task.checkCancellation()
                self.captureTask = nil
                self.presentSelection(frozenCaptures: captures)
            } catch is CancellationError {
                self.captureTask = nil
                self.finishWorkflow()
            } catch {
                self.captureTask = nil
                self.presentError(
                    self.localized(
                        "无法完成截图：\(error.localizedDescription)",
                        "Unable to capture screenshot: \(error.localizedDescription)"
                    )
                )
                self.finishWorkflow()
            }
        }
    }

    private func presentSelection(frozenCaptures: [CGDirectDisplayID: CGImage]) {
        let controller = ScreenSelectionOverlayController(
            isEnglish: model?.interfaceLanguage == .english,
            frozenCaptures: frozenCaptures
        )
        controller.onComplete = { [weak self] selection in
            self?.selectionController = nil
            self?.presentInlineAnnotation(image: selection.image, selection: selection)
        }
        controller.onCancel = { [weak self] in
            self?.selectionController = nil
            self?.finishWorkflow()
        }
        selectionController = controller
        controller.present()
    }

    func cancelCurrentWorkflow() {
        if let selectionController {
            selectionController.cancel()
            return
        }
        if let annotationController {
            annotationController.cancel()
            return
        }
        if let longCaptureController {
            longCaptureController.cancel()
            return
        }
        captureTask?.cancel()
        captureTask = nil
        finishWorkflow()
    }

    private func presentInlineAnnotation(
        image: CGImage,
        selection: ScreenSelection
    ) {
        selectedScreen = selection.screen
        let controller = ScreenshotInlineAnnotationController(
            image: image,
            selection: selection,
            isEnglish: model?.interfaceLanguage == .english
        )
        controller.onComplete = { [weak self] renderedImage in
            guard let self else { return }
            self.annotationController = nil
            let output = self.persist(renderedImage)
            self.toastController.show(
                output: output,
                on: selection.screen,
                isEnglish: self.model?.interfaceLanguage == .english
            )
            self.finishWorkflow()
        }
        controller.onSendToTranslation = { [weak self] renderedImage in
            guard let self else { return }
            self.annotationController = nil
            self.finishWorkflow()
            self.sendToPetTranslation(renderedImage)
        }
        controller.onCancel = { [weak self] in
            self?.annotationController = nil
            self?.finishWorkflow()
        }
        controller.onRequestLongCapture = { [weak self] in
            guard let self else { return }
            self.annotationController = nil
            self.presentLongCapture(initialImage: image, selection: selection)
        }
        annotationController = controller
        controller.present()
    }

    private func presentLongCapture(initialImage: CGImage, selection: ScreenSelection) {
        let controller = LongScreenshotCaptureController(
            initialImage: initialImage,
            selection: selection,
            isEnglish: model?.interfaceLanguage == .english
        )
        controller.onComplete = { [weak self] image in
            guard let self else { return }
            self.longCaptureController = nil
            let output = self.persist(image)
            self.toastController.show(
                output: output,
                on: selection.screen,
                isEnglish: self.model?.interfaceLanguage == .english
            )
            self.finishWorkflow()
        }
        controller.onCancel = { [weak self] in
            self?.longCaptureController = nil
            self?.finishWorkflow()
        }
        longCaptureController = controller
        controller.present()
    }

    private func persist(_ image: CGImage) -> ScreenshotOutput {
        let bitmap = NSBitmapImageRep(cgImage: image)
        let pngData = bitmap.representation(using: .png, properties: [:])
        let copied = Self.copyToPasteboard(image, pngData: pngData)

        guard let pngData else {
            return ScreenshotOutput(
                image: image,
                fileURL: nil,
                copiedToPasteboard: copied,
                errorMessage: localized(
                    "图片编码失败，但仍尝试复制到了剪贴板。",
                    "Image encoding failed, but it was still copied when possible."
                )
            )
        }

        do {
            let directory = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent("Pictures/ChatOS/Screenshots", isDirectory: true)
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true
            )
            let fileURL = directory.appendingPathComponent(Self.screenshotFilename())
            try pngData.write(to: fileURL, options: .atomic)
            return ScreenshotOutput(
                image: image,
                fileURL: fileURL,
                copiedToPasteboard: copied,
                errorMessage: copied ? nil : localized(
                    "图片已保存，但未能写入剪贴板。",
                    "The image was saved but could not be copied to the pasteboard."
                )
            )
        } catch {
            return ScreenshotOutput(
                image: image,
                fileURL: nil,
                copiedToPasteboard: copied,
                errorMessage: localized(
                    "保存失败：\(error.localizedDescription)",
                    "Save failed: \(error.localizedDescription)"
                )
            )
        }
    }

    private func sendToPetTranslation(_ image: CGImage) {
        let bitmap = NSBitmapImageRep(cgImage: image)
        guard let pngData = bitmap.representation(using: .png, properties: [:]) else {
            presentError(localized(
                "无法把截图发送到快速翻译：图片编码失败。",
                "Unable to send the screenshot to Quick Translate: image encoding failed."
            ))
            return
        }
        model?.openPetTranslationImage(
            data: pngData,
            suggestedName: Self.screenshotFilename()
        )
    }

    @discardableResult
    static func copyToPasteboard(_ image: CGImage, pngData: Data? = nil) -> Bool {
        let bitmap = NSBitmapImageRep(cgImage: image)
        guard let png = pngData ?? bitmap.representation(using: .png, properties: [:]) else {
            return false
        }

        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        var types: [NSPasteboard.PasteboardType] = [.png]
        let tiff = bitmap.representation(using: .tiff, properties: [:])
        if tiff != nil {
            types.append(.tiff)
        }
        pasteboard.declareTypes(types, owner: nil)

        let wrotePNG = pasteboard.setData(png, forType: .png)
        if let tiff {
            pasteboard.setData(tiff, forType: .tiff)
        }
        guard wrotePNG else { return false }
        return pasteboard.availableType(from: [.png, .tiff]) != nil
            && pasteboard.data(forType: .png) != nil
    }

    private func ensureScreenCapturePermission() -> Bool {
        if NativeSystemPermissionService.hasScreenCaptureAccess {
            return true
        }
        if NativeSystemPermissionService.requestScreenCaptureAccess() {
            return true
        }

        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = localized(
            "需要屏幕录制权限",
            "Screen Recording Permission Required"
        )
        alert.informativeText = localized(
            "ChatOS 需要此权限才能获取你选择的截图区域。授权后可能需要重新启动 ChatOS。",
            "ChatOS needs this permission to capture the selected region. You may need to restart ChatOS after granting access."
        )
        alert.addButton(withTitle: localized("打开系统设置", "Open System Settings"))
        alert.addButton(withTitle: localized("取消", "Cancel"))
        NSApp.activate(ignoringOtherApps: true)
        if alert.runModal() == .alertFirstButtonReturn {
            NativeSystemPermissionService.openScreenCapturePrivacySettings()
        }
        return false
    }

    private func presentError(_ message: String) {
        let alert = NSAlert()
        alert.alertStyle = .critical
        alert.messageText = localized("截图失败", "Screenshot Failed")
        alert.informativeText = message
        alert.addButton(withTitle: localized("好", "OK"))
        NSApp.activate(ignoringOtherApps: true)
        alert.runModal()
    }

    private func finishWorkflow() {
        selectionController = nil
        annotationController = nil
        longCaptureController = nil
        captureTask = nil
        selectedScreen = nil
        isRunning = false
        Self.workflowGate.release(self)
        restorePreviousApplication()
    }

    private func restorePreviousApplication() {
        previousApplication?.activate()
        previousApplication = nil
    }

    private func localized(_ chinese: String, _ english: String) -> String {
        model?.interfaceLanguage == .english ? english : chinese
    }

    private static func screenshotFilename() -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd 'at' HH.mm.ss"
        return "ChatOS Screenshot \(formatter.string(from: Date())).png"
    }
}

@MainActor
final class ScreenshotWorkflowGate {
    private weak var owner: AnyObject?

    func acquire(_ candidate: AnyObject) -> Bool {
        guard owner == nil else { return false }
        owner = candidate
        return true
    }

    func release(_ candidate: AnyObject) {
        guard owner === candidate else { return }
        owner = nil
    }
}
