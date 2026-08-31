import AppKit
import ChatOSConnector
import Foundation
import SwiftUI

@MainActor
final class ScreenRecordingCoordinator {
    private enum State {
        case idle
        case selecting
        case preparing
        case recording
        case finishing
    }

    private weak var model: AppModel?
    private let service = NativeScreenRecordingService()
    private lazy var pickerViewModel = RecordingTargetPickerViewModel(service: service)
    private let pickerPanel = GlobalCommandPanelController(size: NSSize(width: 620, height: 500))
    private let controlPanel = RecordingControlPanelController()
    private let resultToast = RecordingResultToastController()
    private var state: State = .idle
    private var temporaryOutputURL: URL?

    init(model: AppModel) {
        self.model = model
        pickerViewModel.onStart = { [weak self] target, capturesAudio in
            self?.beginRecording(target: target, capturesSystemAudio: capturesAudio)
        }
        pickerViewModel.onCancel = { [weak self] in
            self?.pickerPanel.closeAndRestorePreviousApplication()
            self?.state = .idle
        }
        pickerPanel.onPanelDismiss = { [weak self] in
            guard self?.state == .selecting else { return }
            self?.state = .idle
        }
        controlPanel.onStop = { [weak self] in
            self?.stopRecording()
        }
    }

    func toggle() {
        switch state {
        case .idle:
            presentTargetPicker()
        case .selecting:
            pickerPanel.closeAndRestorePreviousApplication()
            state = .idle
        case .recording:
            stopRecording()
        case .preparing, .finishing:
            break
        }
    }

    private func presentTargetPicker() {
        guard ensureScreenCapturePermission() else { return }
        state = .selecting
        pickerViewModel.load()
        pickerPanel.setRootView(RecordingTargetPickerView(
            viewModel: pickerViewModel,
            isEnglish: model?.interfaceLanguage == .english
        ))
        pickerPanel.present()
    }

    private func beginRecording(
        target: NativeScreenRecordingTarget,
        capturesSystemAudio: Bool
    ) {
        guard state == .selecting else { return }
        state = .preparing
        pickerPanel.closeAndRestorePreviousApplication()
        let temporaryURL = Self.recordingTemporaryDirectory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathExtension("mov")
        temporaryOutputURL = temporaryURL
        Task { [weak self, service] in
            try? await Task.sleep(for: .milliseconds(300))
            guard let self else { return }
            do {
                try await service.start(
                    target: target,
                    outputURL: temporaryURL,
                    capturesSystemAudio: capturesSystemAudio
                )
                state = .recording
                controlPanel.present(isEnglish: model?.interfaceLanguage == .english)
            } catch {
                state = .idle
                temporaryOutputURL = nil
                presentError(
                    localized("无法开始录屏", "Unable to Start Recording"),
                    detail: error.localizedDescription
                )
            }
        }
    }

    private func stopRecording() {
        guard state == .recording else { return }
        state = .finishing
        controlPanel.dismiss()
        Task { [weak self, service] in
            guard let self else { return }
            do {
                let temporaryURL = try await service.stop()
                let finalURL = try moveToRecordingDirectory(temporaryURL)
                state = .idle
                temporaryOutputURL = nil
                resultToast.present(
                    url: finalURL,
                    isEnglish: model?.interfaceLanguage == .english
                )
            } catch {
                state = .idle
                presentError(
                    localized("录屏保存失败", "Recording Save Failed"),
                    detail: error.localizedDescription
                )
            }
        }
    }

    private func moveToRecordingDirectory(_ temporaryURL: URL) throws -> URL {
        let directory = FileManager.default.urls(for: .moviesDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("ChatOS", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let finalURL = directory.appendingPathComponent(Self.recordingFilename())
        try FileManager.default.moveItem(at: temporaryURL, to: finalURL)
        return finalURL
    }

    private func ensureScreenCapturePermission() -> Bool {
        if NativeSystemPermissionService.hasScreenCaptureAccess
            || NativeSystemPermissionService.requestScreenCaptureAccess() {
            return true
        }
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = localized(
            "需要屏幕录制权限",
            "Screen Recording Permission Required"
        )
        alert.informativeText = localized(
            "ChatOS 需要屏幕录制权限才能录制显示器或窗口。授权后可能需要重新启动客户端。",
            "ChatOS needs Screen Recording access to record a display or window. You may need to restart the app after granting access."
        )
        alert.addButton(withTitle: localized("打开系统设置", "Open System Settings"))
        alert.addButton(withTitle: localized("取消", "Cancel"))
        NSApp.activate(ignoringOtherApps: true)
        if alert.runModal() == .alertFirstButtonReturn {
            NativeSystemPermissionService.openScreenCapturePrivacySettings()
        }
        return false
    }

    private func presentError(_ title: String, detail: String) {
        let alert = NSAlert()
        alert.alertStyle = .critical
        alert.messageText = title
        alert.informativeText = detail
        alert.addButton(withTitle: localized("好", "OK"))
        NSApp.activate(ignoringOtherApps: true)
        alert.runModal()
    }

    private func localized(_ chinese: String, _ english: String) -> String {
        model?.interfaceLanguage == .english ? english : chinese
    }

    private static var recordingTemporaryDirectory: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("ChatOS", isDirectory: true)
            .appendingPathComponent("RecordingTemp", isDirectory: true)
    }

    private static func recordingFilename() -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd 'at' HH.mm.ss"
        return "ChatOS Recording \(formatter.string(from: Date())).mov"
    }
}
