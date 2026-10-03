import AppKit
import ChatOSConnector
import CoreGraphics
import Foundation
import ImageIO

enum ScreenshotPersistencePolicy {
    static let maximumPasteboardBytes = 128 * 1_024 * 1_024
    static let maximumTranslationBytes = 20 * 1_024 * 1_024
}

private struct PreparedScreenshotPersistence: Sendable {
    let fileURL: URL?
    let pngData: Data?
    let errorDescription: String?
}

private enum ScreenshotPersistenceError: LocalizedError {
    case encodingFailed

    var errorDescription: String? {
        "The screenshot could not be encoded as PNG."
    }
}

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
    private var finalizationTask: Task<Void, Never>?
    private var finalizationGeneration: UUID?
    private var translationTask: Task<Void, Never>?
    private var translationGeneration: UUID?
    private var previousApplication: NSRunningApplication?
    private var selectedScreen: NSScreen?
    private(set) var isRunning = false

    init(model: AppModel) {
        self.model = model
    }

    deinit {
        captureTask?.cancel()
        finalizationTask?.cancel()
        translationTask?.cancel()
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
        if finalizationTask != nil {
            finalizationTask?.cancel()
            finalizationTask = nil
            finalizationGeneration = nil
            finishWorkflow()
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
            self.finalizeScreenshot(renderedImage, on: selection.screen)
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
            self.finalizeScreenshot(image, on: selection.screen)
        }
        controller.onCancel = { [weak self] in
            self?.longCaptureController = nil
            self?.finishWorkflow()
        }
        longCaptureController = controller
        controller.present()
    }

    private func finalizeScreenshot(_ image: CGImage, on screen: NSScreen) {
        let generation = UUID()
        finalizationGeneration = generation
        finalizationTask = Task { [weak self] in
            guard let self else { return }
            let output = await persist(image)
            guard !Task.isCancelled, finalizationGeneration == generation else { return }
            finalizationTask = nil
            finalizationGeneration = nil
            toastController.show(
                output: output,
                on: screen,
                isEnglish: model?.interfaceLanguage == .english
            )
            finishWorkflow()
        }
    }

    private func persist(_ image: CGImage) async -> ScreenshotOutput {
        let directory = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Pictures/ChatOS/Screenshots", isDirectory: true)
        let fileURL = directory.appendingPathComponent(Self.screenshotFilename())
        let prepared = (try? await AppCancellableDetachedWork.run {
            Self.persistImage(image, to: fileURL)
        }) ?? PreparedScreenshotPersistence(
            fileURL: nil,
            pngData: nil,
            errorDescription: CancellationError().localizedDescription
        )
        guard let savedURL = prepared.fileURL else {
            return ScreenshotOutput(
                pngData: nil,
                fileURL: nil,
                copiedToPasteboard: false,
                errorMessage: localized(
                    "保存失败：\(prepared.errorDescription ?? "图片编码失败")",
                    "Save failed: \(prepared.errorDescription ?? "image encoding failed")"
                )
            )
        }

        let copied = prepared.pngData.map(Self.copyPNGToPasteboard) ?? false
        return ScreenshotOutput(
            pngData: prepared.pngData,
            fileURL: savedURL,
            copiedToPasteboard: copied,
            errorMessage: copied ? nil : localized(
                "图片已保存，但文件过大或未能写入剪贴板。",
                "The image was saved, but it was too large or could not be copied to the pasteboard."
            )
        )
    }

    private func sendToPetTranslation(_ image: CGImage) {
        translationTask?.cancel()
        let generation = UUID()
        let suggestedName = Self.screenshotFilename()
        translationGeneration = generation
        translationTask = Task { [weak self] in
            guard let self else { return }
            let pngData = try? await AppCancellableDetachedWork.run {
                Self.encodePNGData(
                    image,
                    maximumBytes: ScreenshotPersistencePolicy.maximumTranslationBytes
                )
            }
            guard !Task.isCancelled, translationGeneration == generation else { return }
            translationTask = nil
            translationGeneration = nil
            guard let pngData else {
                presentError(localized(
                    "无法把截图发送到快速翻译：PNG 编码失败或文件超过 20 MB。",
                    "Unable to send the screenshot to Quick Translate: PNG encoding failed or exceeded 20 MB."
                ))
                return
            }
            model?.openPetTranslationImage(data: pngData, suggestedName: suggestedName)
        }
    }

    @MainActor @discardableResult
    static func copyPNGToPasteboard(_ pngData: Data) -> Bool {
        guard !pngData.isEmpty,
              pngData.count <= ScreenshotPersistencePolicy.maximumPasteboardBytes else {
            return false
        }
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.declareTypes([.png], owner: nil)
        let wrotePNG = pasteboard.setData(pngData, forType: .png)
        guard wrotePNG else { return false }
        return pasteboard.availableType(from: [.png]) != nil
            && pasteboard.data(forType: .png) != nil
    }

    nonisolated private static func persistImage(
        _ image: CGImage,
        to fileURL: URL
    ) -> PreparedScreenshotPersistence {
        do {
            try Task.checkCancellation()
            try FileManager.default.createDirectory(
                at: fileURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            guard let destination = CGImageDestinationCreateWithURL(
                fileURL as CFURL,
                "public.png" as CFString,
                1,
                nil
            ) else {
                throw ScreenshotPersistenceError.encodingFailed
            }
            CGImageDestinationAddImage(destination, image, nil)
            try Task.checkCancellation()
            guard CGImageDestinationFinalize(destination) else {
                throw ScreenshotPersistenceError.encodingFailed
            }
            try Task.checkCancellation()
            let pngData = try? AppBoundedFileReader.read(
                fileURL,
                maximumBytes: ScreenshotPersistencePolicy.maximumPasteboardBytes
            )
            return PreparedScreenshotPersistence(
                fileURL: fileURL,
                pngData: pngData,
                errorDescription: nil
            )
        } catch {
            try? FileManager.default.removeItem(at: fileURL)
            return PreparedScreenshotPersistence(
                fileURL: nil,
                pngData: nil,
                errorDescription: error.localizedDescription
            )
        }
    }

    nonisolated private static func encodePNGData(
        _ image: CGImage,
        maximumBytes: Int
    ) -> Data? {
        guard maximumBytes > 0, !Task.isCancelled else { return nil }
        let output = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(
            output,
            "public.png" as CFString,
            1,
            nil
        ) else {
            return nil
        }
        CGImageDestinationAddImage(destination, image, nil)
        guard !Task.isCancelled,
              CGImageDestinationFinalize(destination),
              !Task.isCancelled,
              output.length > 0,
              output.length <= maximumBytes else {
            return nil
        }
        return Data(referencing: output)
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
