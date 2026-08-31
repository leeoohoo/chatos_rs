import Foundation

@MainActor
final class AppIdleSleepController {
    private var activity: NSObjectProtocol?

    var isEnabled: Bool { activity != nil }

    func setEnabled(_ enabled: Bool) {
        guard enabled != isEnabled else { return }
        if enabled {
            activity = ProcessInfo.processInfo.beginActivity(
                options: [.userInitiated, .idleSystemSleepDisabled],
                reason: "ChatOS 用户已开启程序运行期间保活"
            )
        } else if let activity {
            ProcessInfo.processInfo.endActivity(activity)
            self.activity = nil
        }
    }
}
