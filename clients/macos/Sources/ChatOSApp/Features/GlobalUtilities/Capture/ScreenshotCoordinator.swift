import AppKit
import ChatOSConnector
import CoreGraphics
import Foundation

@MainActor
final class ScreenshotCoordinator {
    private weak var model: AppModel?
    private let captureService = NativeScreenCaptureService()
    private let toastController = CaptureResultToastController()

    private var selectionController: ScreenSelectionOverlayController?
    private var annotationEditor: ScreenshotAnnotationEditorController?
    private var captureTask: Task<Void, Never>?
    private var previousApplication: NSRunningApplication?
    private var selectedScreen: NSScreen?
    private(set) var isRunning = false

    init(model: AppModel) {
        self.model = model
    }

    func start() {
        if isRunning {
            cancelCurrentWorkflow()
            return
        }

        if NSWorkspace.shared.frontmostApplication?.bundleIdentifier
            != Bundle.main.bundleIdentifier {
            previousApplication = NSWorkspace.shared.frontmostApplication
        }

        guard ensureScreenCapturePermission() else {
            restorePreviousApplication()
            return
        }

        isRunning = true
        let controller = ScreenSelectionOverlayController(
            isEnglish: model?.interfaceLanguage == .english
        )
        controller.onComplete = { [weak self] selection in
            self?.selectionController = nil
            self?.capture(selection)
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
        if let annotationEditor {
            annotationEditor.cancel()
            return
        }
        captureTask?.cancel()
        captureTask = nil
        finishWorkflow()
    }

    private func capture(_ selection: ScreenSelection) {
        selectedScreen = selection.screen
        captureTask?.cancel()
        captureTask = Task { [weak self] in
            guard let self else { return }
            do {
                let image = try await self.captureService.capture(region: selection.captureRegion)
                try Task.checkCancellation()
                self.captureTask = nil
                self.presentAnnotationEditor(image: image, screen: selection.screen)
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

    private func presentAnnotationEditor(image: CGImage, screen: NSScreen) {
        let editor = ScreenshotAnnotationEditorController(
            image: image,
            screen: screen,
            isEnglish: model?.interfaceLanguage == .english
        )
        editor.onComplete = { [weak self] renderedImage in
            guard let self else { return }
            self.annotationEditor = nil
            let output = self.persist(renderedImage)
            self.toastController.show(
                output: output,
                on: screen,
                isEnglish: self.model?.interfaceLanguage == .english
            )
            self.finishWorkflow()
        }
        editor.onCancel = { [weak self] in
            self?.annotationEditor = nil
            self?.finishWorkflow()
        }
        annotationEditor = editor
        editor.present()
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

    @discardableResult
    static func copyToPasteboard(_ image: CGImage, pngData: Data? = nil) -> Bool {
        let bitmap = NSBitmapImageRep(cgImage: image)
        guard let png = pngData ?? bitmap.representation(using: .png, properties: [:]),
              let tiff = bitmap.representation(using: .tiff, properties: [:]) else {
            return false
        }

        let item = NSPasteboardItem()
        item.setData(png, forType: .png)
        item.setData(tiff, forType: .tiff)
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        return pasteboard.writeObjects([item])
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
        annotationEditor = nil
        captureTask = nil
        selectedScreen = nil
        isRunning = false
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
