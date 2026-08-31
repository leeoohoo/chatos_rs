import ChatOSCore
import Foundation

@MainActor
final class GlobalUtilityPreferencesStore: ObservableObject {
    private enum Key {
        static let enabled = "ChatOS.globalUtilities.enabled"
        static let acknowledgedShortcutConflicts =
            "ChatOS.globalUtilities.acknowledgedShortcutConflicts"
        static let screenshotEnabled = "ChatOS.globalUtilities.screenshot.enabled"
        static let recordingEnabled = "ChatOS.globalUtilities.recording.enabled"
        static let clipboardEnabled = "ChatOS.globalUtilities.clipboard.enabled"
        static let quickSearchEnabled = "ChatOS.globalUtilities.quickSearch.enabled"
        static let hotKeyPrefix = "ChatOS.globalUtilities.hotKey."
    }

    @Published var isEnabled: Bool {
        didSet {
            defaults.set(isEnabled, forKey: Key.enabled)
            configurationDidChange()
        }
    }

    @Published var hasAcknowledgedShortcutConflicts: Bool {
        didSet {
            defaults.set(
                hasAcknowledgedShortcutConflicts,
                forKey: Key.acknowledgedShortcutConflicts
            )
        }
    }

    @Published var screenshotEnabled: Bool {
        didSet {
            defaults.set(screenshotEnabled, forKey: Key.screenshotEnabled)
            configurationDidChange()
        }
    }

    @Published var recordingEnabled: Bool {
        didSet {
            defaults.set(recordingEnabled, forKey: Key.recordingEnabled)
            configurationDidChange()
        }
    }

    @Published var clipboardEnabled: Bool {
        didSet {
            defaults.set(clipboardEnabled, forKey: Key.clipboardEnabled)
            configurationDidChange()
        }
    }

    @Published var quickSearchEnabled: Bool {
        didSet {
            defaults.set(quickSearchEnabled, forKey: Key.quickSearchEnabled)
            configurationDidChange()
        }
    }

    @Published private(set) var hotKeys: [GlobalUtilityAction: GlobalHotKey]
    @Published private(set) var configurationRevision = UUID()

    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        self.isEnabled = defaults.object(forKey: Key.enabled) as? Bool ?? false
        self.hasAcknowledgedShortcutConflicts = defaults.bool(
            forKey: Key.acknowledgedShortcutConflicts
        )
        self.screenshotEnabled = defaults.object(forKey: Key.screenshotEnabled) as? Bool ?? true
        self.recordingEnabled = defaults.object(forKey: Key.recordingEnabled) as? Bool ?? true
        self.clipboardEnabled = defaults.object(forKey: Key.clipboardEnabled) as? Bool ?? true
        self.quickSearchEnabled = defaults.object(forKey: Key.quickSearchEnabled) as? Bool ?? true
        self.hotKeys = Dictionary(uniqueKeysWithValues: GlobalUtilityAction.allCases.map { action in
            (action, Self.loadHotKey(action, defaults: defaults) ?? action.defaultHotKey)
        })
    }

    func hotKey(for action: GlobalUtilityAction) -> GlobalHotKey {
        hotKeys[action] ?? action.defaultHotKey
    }

    func isActionEnabled(_ action: GlobalUtilityAction) -> Bool {
        switch action {
        case .screenshot: screenshotEnabled
        case .screenRecording: recordingEnabled
        case .clipboardHistory: clipboardEnabled
        case .quickSearch: quickSearchEnabled
        }
    }

    func setHotKey(_ hotKey: GlobalHotKey, for action: GlobalUtilityAction) {
        guard hotKey.isValid else { return }
        hotKeys[action] = hotKey
        if let encoded = try? JSONEncoder().encode(hotKey) {
            defaults.set(encoded, forKey: Self.hotKeyKey(action))
        }
        configurationDidChange()
    }

    func setActionEnabled(_ enabled: Bool, for action: GlobalUtilityAction) {
        switch action {
        case .screenshot: screenshotEnabled = enabled
        case .screenRecording: recordingEnabled = enabled
        case .clipboardHistory: clipboardEnabled = enabled
        case .quickSearch: quickSearchEnabled = enabled
        }
    }

    func restoreDefaults() {
        for action in GlobalUtilityAction.allCases {
            defaults.removeObject(forKey: Self.hotKeyKey(action))
        }
        hotKeys = Dictionary(uniqueKeysWithValues: GlobalUtilityAction.allCases.map {
            ($0, $0.defaultHotKey)
        })
        screenshotEnabled = true
        recordingEnabled = true
        clipboardEnabled = true
        quickSearchEnabled = true
        configurationDidChange()
    }

    private func configurationDidChange() {
        configurationRevision = UUID()
    }

    private static func hotKeyKey(_ action: GlobalUtilityAction) -> String {
        Key.hotKeyPrefix + action.rawValue
    }

    private static func loadHotKey(
        _ action: GlobalUtilityAction,
        defaults: UserDefaults
    ) -> GlobalHotKey? {
        guard let data = defaults.data(forKey: hotKeyKey(action)) else { return nil }
        return try? JSONDecoder().decode(GlobalHotKey.self, from: data)
    }
}
